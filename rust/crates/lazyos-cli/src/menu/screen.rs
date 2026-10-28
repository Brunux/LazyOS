//! The menu window: the list, the power confirm, and the Ask strip, as modes
//! of one [`Screen`] so the window never closes into another program halfway
//! through a choice.
//!
//! ```text
//! List ──Enter──▶ (a program)                       the window closes, then it runs
//!   │ ──Enter on Power──▶ Power ──Enter──▶ Confirm   Cancel first; Esc goes back
//!   │ ──Enter on Ask────▶ Ask prompt ──Enter──▶ Answer (a task: spec 005)
//! ```
//!
//! A chosen program is not started from inside the window: the screen records
//! it ([`MenuScreen::chosen`]) and the caller runs it once the terminal is
//! restored, which is the order `bar-menu`'s `exec` had.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use lazyos_ui::app::exit::{ESCAPE, OK};
use lazyos_ui::widgets::{put, put_trunc};
use lazyos_ui::{
    BodyCtx, ColorRole, Confirm, ConfirmAction, Design, Glyph, Header, Input, InputAction,
    KeyHint, ListAction, ListState, Outcome, Row, Screen, Task, TaskState, TaskView, TextStyle,
    draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use super::frecency::Frecency;
use super::{Action, Choice, Context, Entry, SessionAction, Surface, table};
use crate::ask::{self, Agent};

/// What the window decided, for the caller to do once it is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// Start this program detached.
    Run { key: String, argv: Vec<String> },
    /// A new password onto the clipboard, through the secret card.
    Password { key: String },
    /// Log out, reboot or shut down - only ever set by the confirm's second
    /// answer.
    Session(SessionAction),
}

/// Where the window is.
enum Mode {
    List,
    Power,
    Confirm(SessionAction, Confirm),
    Ask(Input),
    Answer(Box<Answer>),
}

/// Everything the window reads from the session, injected so a test can drive
/// it without one.
pub struct Setup {
    pub surface: Surface,
    pub context: Context,
    pub vars: BTreeMap<String, String>,
    /// This binary, for the Ask task's child and the palette's `lazyos` rows.
    pub exe: String,
    /// `~/.local/state/lazyos`: the frecency file and the task records.
    pub state: PathBuf,
    /// The agent that would answer a question, resolved once when the window
    /// opens. `None` when neither wired agent is installed.
    pub agent: Option<ask::Choice>,
    /// Open on the Ask prompt (`lazyos ask`), with this text in it.
    pub ask: Option<String>,
}

pub struct MenuScreen {
    setup: Setup,
    entries: Vec<Entry>,
    list: ListState,
    power: ListState,
    mode: Mode,
    header: Header,
    keybar: Vec<KeyHint>,
    /// Set when the window ends on a choice; the caller acts on it.
    pub chosen: Option<Picked>,
}

impl MenuScreen {
    pub fn new(setup: Setup) -> Self {
        let entries = super::entries(setup.surface, &setup.context);
        let frecency = Frecency::load(super::frecency_path(&setup.state));
        let now = super::now();
        let mut list = ListState::new()
            .with_empty("nothing matches - Backspace widens it")
            .ranked();
        list.set_rows(
            entries
                .iter()
                .map(|entry| entry.row(frecency.boost(&entry.key, now)))
                .collect(),
        );
        let mut power = ListState::new();
        power.set_rows(
            table::POWER
                .iter()
                .map(|item| Row::new(item.label).glyph(item.glyph).detail(item.detail))
                .collect(),
        );
        let mode = match &setup.ask {
            Some(text) => Mode::Ask(Input::with_text(text.clone())),
            None => Mode::List,
        };
        let mut screen = Self {
            setup,
            entries,
            list,
            power,
            mode,
            header: Header::new("LazyOS"),
            keybar: Vec::new(),
            chosen: None,
        };
        screen.refresh();
        screen
    }

    /// The header and the key bar for the current mode.
    fn refresh(&mut self) {
        let (subtitle, status, keys): (&str, String, Vec<(&str, &str)>) = match &self.mode {
            Mode::List => (
                match self.setup.surface {
                    Surface::Root => "Menu",
                    Surface::Palette => "Palette",
                },
                format!("{} rows", self.list.visible_len()),
                vec![
                    ("Enter", "open"),
                    ("type", "filter"),
                    ("\u{2191}\u{2193}", "move"),
                    ("Esc", "close"),
                ],
            ),
            Mode::Power => (
                "Power",
                "each asks first".to_string(),
                vec![("Enter", "choose"), ("Esc", "back")],
            ),
            Mode::Confirm(..) => (
                "Power",
                "Cancel is selected".to_string(),
                vec![
                    ("\u{2190}\u{2192}", "choose"),
                    ("Enter", "answer"),
                    ("Esc", "cancel"),
                ],
            ),
            Mode::Ask(_) => (
                "AI \u{00b7} Ask",
                self.agent_name().to_string(),
                vec![("Enter", "ask"), ("Esc", "back")],
            ),
            Mode::Answer(answer) => ("AI \u{00b7} Ask", answer.status(), answer.keys()),
        };
        self.header = Header::new("LazyOS").subtitle(subtitle).status(status);
        self.keybar = keys
            .into_iter()
            .map(|(key, label)| KeyHint::new(key, label))
            .collect();
    }

    fn agent_name(&self) -> &'static str {
        self.setup
            .agent
            .as_ref()
            .map_or("no agent", |choice| choice.agent.name())
    }

    /// The list's two areas: the filter line and the rows. `draw` and `hit`
    /// both call this, so a click lands on the row that was drawn under it.
    fn list_layout(area: Rect) -> (Rect, Rect) {
        let inner = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(2),
        };
        let filter = Rect {
            height: 1.min(inner.height),
            ..inner
        };
        let rows = Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        };
        (filter, rows)
    }

    fn activate(&mut self, index: usize) -> Outcome {
        let Some(entry) = self.entries.get(index).cloned() else {
            return Outcome::Continue;
        };
        match entry.action {
            Choice::Table(Action::Power) => {
                self.power.set_cursor(0);
                self.mode = Mode::Power;
                Outcome::Continue
            }
            Choice::Table(Action::Ask) => {
                super::remember(&self.setup.state, &entry.key);
                self.mode = Mode::Ask(Input::new());
                Outcome::Continue
            }
            Choice::Table(Action::Password) => {
                self.chosen = Some(Picked::Password { key: entry.key });
                Outcome::Done(OK)
            }
            Choice::Table(Action::Session(_)) => Outcome::Continue,
            choice => match super::argv(choice, &self.setup.vars, &self.setup.exe) {
                Some(argv) => {
                    self.chosen = Some(Picked::Run {
                        key: entry.key,
                        argv,
                    });
                    Outcome::Done(OK)
                }
                None => Outcome::Continue,
            },
        }
    }

    /// Start the question as a task: the record first (with the child's argv),
    /// then the detached runner, then the view that watches it.
    fn ask(&mut self, question: String) -> Outcome {
        let Some(choice) = self.setup.agent.clone() else {
            self.mode = Mode::Answer(Box::new(Answer::refused(
                "No agent that can answer one question is installed: the Agents menu installs Claude Code or OpenCode",
            )));
            return Outcome::Continue;
        };
        let id = format!("ask-{}-{}", super::now(), std::process::id());
        let title = format!("Asking {}", choice.agent.name());
        let mut task = Task::new(&id, &title, &self.setup.state).with_command(&[
            &self.setup.exe,
            "--ask-run",
            &id,
            choice.agent.bin(),
            &question,
        ]);
        if let Some(runtime) = self.setup.vars.get("XDG_RUNTIME_DIR") {
            task = task.with_runtime_dir(runtime).with_busy_icon("dialog-question");
        }
        task.set_stoppable(true);
        task.add_step(title.clone());
        let started = task
            .write_state()
            .and_then(|_| crate::task_cmd::spawn_runner(&id, &self.setup.exe, &[]));
        self.mode = Mode::Answer(Box::new(match started {
            Ok(()) => Answer::new(task, choice.agent, question),
            Err(error) => Answer::refused(&format!("could not start the question: {error}")),
        }));
        Outcome::Continue
    }

    fn draw_list(&mut self, ctx: &mut BodyCtx) {
        let title = match self.setup.surface {
            Surface::Root => "Menu",
            Surface::Palette => "Palette",
        };
        draw_card(ctx.buf, ctx.area, ctx.design, Some(title), true);
        let (filter, rows) = Self::list_layout(ctx.area);
        let x = put(
            ctx.buf,
            filter.x,
            filter.y,
            Glyph::Search.cell(),
            filter.width,
            ctx.design.style(TextStyle::Title),
        );
        let x = put(ctx.buf, x, filter.y, " ", 1, ctx.design.style(TextStyle::Body));
        let room = filter.right().saturating_sub(x);
        if self.list.filter().is_empty() {
            put_trunc(
                ctx.buf,
                x,
                filter.y,
                "type to filter: a name, a word of it, or `system`",
                room,
                ctx.design.style(TextStyle::Dim),
            );
        } else {
            let end = put_trunc(
                ctx.buf,
                x,
                filter.y,
                self.list.filter(),
                room.saturating_sub(1),
                ctx.design.style(TextStyle::Body),
            );
            put(ctx.buf, end, filter.y, " ", 1, ctx.design.style(TextStyle::Key));
        }
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn draw_power(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Power"), true);
        let (filter, rows) = Self::list_layout(ctx.area);
        put_trunc(
            ctx.buf,
            filter.x,
            filter.y,
            "Each of these ends the session, and asks once more first.",
            filter.width,
            ctx.design.style(TextStyle::Dim),
        );
        self.power.render(rows, ctx.buf, ctx.design, true);
    }

    fn draw_ask(&self, ctx: &mut BodyCtx, input: &Input) {
        let card = lazyos_ui::centered(ctx.area, 88, 11);
        let inner = draw_card(ctx.buf, card, ctx.design, Some("Ask \u{00b7} AI"), true);
        let width = inner.width;
        let mut y = inner.y;
        let line = |buf: &mut ratatui::buffer::Buffer, y: u16, text: &str, style| {
            put_trunc(buf, inner.x, y, text, width, style);
        };
        match &self.setup.agent {
            Some(choice) => {
                line(
                    ctx.buf,
                    y,
                    &format!("One question to {}; the answer appears here.", choice.agent.name()),
                    ctx.design.style(TextStyle::Body),
                );
                y += 1;
                let sent = match choice.agent {
                    Agent::Claude => "Sends your question and nothing else: no files, no tools, no saved session.",
                    Agent::OpenCode => "Sends your question and nothing else: it runs in an empty directory.",
                };
                line(ctx.buf, y, sent, ctx.design.style(TextStyle::Dim));
                y += 1;
                if let Some(note) = &choice.note {
                    line(
                        ctx.buf,
                        y,
                        note,
                        ctx.design
                            .style(TextStyle::Body)
                            .fg(ctx.design.color(ColorRole::Warn)),
                    );
                }
            }
            None => line(
                ctx.buf,
                y,
                "No agent with a one-question mode is installed (Claude Code or OpenCode).",
                ctx.design.style(TextStyle::Danger),
            ),
        }
        let field = Rect {
            x: inner.x,
            y: inner.y + 4,
            width,
            height: 1,
        };
        input.render(field, ctx.buf, ctx.design, "What do you want to know?");
        line(
            ctx.buf,
            inner.y + 6,
            "Enter asks \u{00b7} Esc goes back \u{00b7} the question costs what the agent's account charges",
            ctx.design.style(TextStyle::Dim),
        );
    }
}

impl Screen for MenuScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        match &mut self.mode {
            Mode::List => self.draw_list(ctx),
            Mode::Power => self.draw_power(ctx),
            Mode::Confirm(_, confirm) => {
                // The list stays behind the question, so the person sees what
                // they were choosing from.
                self.draw_power(ctx);
                confirm.render(ctx.area, ctx.buf, ctx.design);
            }
            Mode::Ask(input) => {
                let input = input.clone();
                self.draw_ask(ctx, &input);
            }
            Mode::Answer(answer) => answer.draw(ctx),
        }
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let rows_height = |area_height: u16| area_height.saturating_sub(4);
        let outcome = match &mut self.mode {
            Mode::List => {
                let typed = matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
                    && !key.modifiers.contains(KeyModifiers::CONTROL);
                if typed {
                    self.list.type_filter(key);
                    Outcome::Continue
                } else {
                    match self.list.handle(key, rows_height(40), design) {
                        ListAction::Activated(index) => self.activate(index),
                        _ => Outcome::Continue,
                    }
                }
            }
            Mode::Power => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::List;
                    Outcome::Continue
                }
                _ => match self.power.handle(key, 10, design) {
                    ListAction::Activated(index) => {
                        if let Some(item) = table::POWER.get(index)
                            && let Action::Session(action) = item.action
                        {
                            let confirm = Confirm::new("Power", action.question())
                                .with_options(&["Cancel", action.verb()]);
                            self.mode = Mode::Confirm(action, confirm);
                        }
                        Outcome::Continue
                    }
                    _ => Outcome::Continue,
                },
            },
            Mode::Confirm(action, confirm) => {
                let action = *action;
                match confirm.handle(key) {
                    ConfirmAction::Chosen(1) => {
                        self.chosen = Some(Picked::Session(action));
                        Outcome::Done(OK)
                    }
                    ConfirmAction::Chosen(_) | ConfirmAction::Cancelled => {
                        self.mode = Mode::Power;
                        Outcome::Continue
                    }
                    ConfirmAction::None => Outcome::Continue,
                }
            }
            Mode::Ask(input) => match input.handle(key) {
                InputAction::Submitted => {
                    let question = input.text().trim().to_string();
                    self.ask(question)
                }
                InputAction::Cancelled => {
                    if self.setup.ask.is_some() {
                        // Opened as `lazyos ask`: there is no menu to go back to.
                        Outcome::Done(ESCAPE)
                    } else {
                        self.mode = Mode::List;
                        Outcome::Continue
                    }
                }
                _ => Outcome::Continue,
            },
            Mode::Answer(answer) => answer.key(key, design),
        };
        self.refresh();
        outcome
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        let outcome = match (&self.mode, row) {
            (Mode::List, Some(visible)) => {
                self.list.set_cursor(visible);
                match self.list.selected_index() {
                    Some(index) => self.activate(index),
                    None => Outcome::Continue,
                }
            }
            _ => Outcome::Continue,
        };
        self.refresh();
        outcome
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        match self.mode {
            Mode::List => {
                let (_, rows) = Self::list_layout(area);
                self.list.hit_row(rows, x, y, design)
            }
            _ => None,
        }
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }

    fn needs_tick(&self) -> Option<Duration> {
        match &self.mode {
            Mode::Answer(answer) => answer.needs_tick(),
            _ => None,
        }
    }

    fn tick(&mut self) -> Outcome {
        let outcome = match &mut self.mode {
            Mode::Answer(answer) => answer.tick(),
            _ => Outcome::Continue,
        };
        self.refresh();
        outcome
    }
}

// ---------------------------------------------------------------------------
// The answer: a task while the agent works, a card when it has answered
// ---------------------------------------------------------------------------

pub struct Answer {
    view: Option<TaskView>,
    agent: Option<Agent>,
    question: String,
    /// The answer wrapped to the width it was last drawn at, and that width:
    /// rebuilt only when either changes, never per frame.
    wrapped: Vec<String>,
    wrapped_for: (u16, usize),
    scroll: usize,
    message: Option<String>,
    refused: Option<String>,
}

impl Answer {
    fn new(task: Task, agent: Agent, question: String) -> Self {
        Self {
            view: Some(TaskView::new(task)),
            agent: Some(agent),
            question,
            wrapped: Vec::new(),
            wrapped_for: (0, 0),
            scroll: 0,
            message: None,
            refused: None,
        }
    }

    fn refused(reason: &str) -> Self {
        Self {
            view: None,
            agent: None,
            question: String::new(),
            wrapped: Vec::new(),
            wrapped_for: (0, 0),
            scroll: 0,
            message: None,
            refused: Some(reason.to_string()),
        }
    }

    fn state(&self) -> Option<TaskState> {
        self.view.as_ref().map(|v| v.task().state)
    }

    fn running(&self) -> bool {
        matches!(self.state(), Some(TaskState::Queued | TaskState::Running))
    }

    /// The answer's text: the task's log, which is what `lazyos --ask-run`
    /// printed - the answer, one line per line.
    fn text(&self) -> String {
        self.view
            .as_ref()
            .map(|v| v.task().log.lines().join("\n"))
            .unwrap_or_default()
    }

    /// The agent's own error line, without the task protocol's `ERROR:` mark.
    fn error(&self) -> String {
        let task = self.view.as_ref().map(|v| v.task());
        task.and_then(|t| t.error_line.clone())
            .map(|line| line.trim_start_matches("ERROR:").trim().to_string())
            .or_else(|| task.and_then(|t| t.result.clone()))
            .unwrap_or_else(|| "the question did not get an answer".to_string())
    }

    fn status(&self) -> String {
        match (&self.refused, self.state()) {
            (Some(_), _) => "refused".to_string(),
            (_, Some(TaskState::Done)) => "answered".to_string(),
            (_, Some(TaskState::Failed | TaskState::DidNotFinish)) => "failed".to_string(),
            (_, Some(TaskState::Stopped)) => "stopped".to_string(),
            _ => self
                .agent
                .map(|a| format!("{} is answering", a.name()))
                .unwrap_or_default(),
        }
    }

    fn keys(&self) -> Vec<(&'static str, &'static str)> {
        if self.running() {
            vec![("s", "stop"), ("Esc", "stop and close")]
        } else if self.state() == Some(TaskState::Done) {
            vec![("c", "copy"), ("\u{2191}\u{2193}", "scroll"), ("Esc", "close")]
        } else {
            vec![("Esc", "close")]
        }
    }

    fn needs_tick(&self) -> Option<Duration> {
        self.running().then_some(Duration::from_millis(100))
    }

    /// The viewer polls the runner's record: the task may still be `Queued`
    /// when the window draws first, which `TaskView` alone would never poll.
    fn tick(&mut self) -> Outcome {
        if let Some(view) = self.view.as_mut()
            && matches!(view.task().state, TaskState::Queued | TaskState::Running)
        {
            let _ = view.task_mut().reload();
            let _ = view.tick();
        }
        Outcome::Continue
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if self.running() {
            return match key.code {
                // Closing the window while the agent works stops it: the runner
                // forwards the stop, and `--ask-run` kills the agent's group.
                KeyCode::Esc | KeyCode::Char('q') => {
                    if let Some(view) = &self.view {
                        view.task().request_stop();
                    }
                    Outcome::Done(ESCAPE)
                }
                _ => match &mut self.view {
                    Some(view) => view.key(key, design),
                    None => Outcome::Continue,
                },
            };
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Done(
                if self.state() == Some(TaskState::Done) {
                    OK
                } else {
                    ESCAPE
                },
            ),
            KeyCode::Char('c') if self.state() == Some(TaskState::Done) => {
                self.message = Some(match lazyos_ui::card::copy_to_clipboard(&self.text()) {
                    Ok(()) => "Copied to the clipboard".to_string(),
                    Err(_) => "Could not copy: wl-copy did not take it".to_string(),
                });
                Outcome::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                Outcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = (self.scroll + 1).min(self.wrapped.len().saturating_sub(1));
                Outcome::Continue
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                Outcome::Continue
            }
            KeyCode::PageDown => {
                self.scroll = (self.scroll + 10).min(self.wrapped.len().saturating_sub(1));
                Outcome::Continue
            }
            KeyCode::Home => {
                self.scroll = 0;
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }

    fn draw(&mut self, ctx: &mut BodyCtx) {
        if let Some(reason) = &self.refused {
            lazyos_ui::draw_failure(ctx.buf, ctx.area, ctx.design, "Nothing was asked", reason);
            return;
        }
        if self.running() {
            if let Some(view) = self.view.as_mut() {
                view.draw(ctx);
            }
            return;
        }
        let failed = self.state() != Some(TaskState::Done);
        let title = self.agent.map_or("Answer", Agent::name);
        let inner = draw_card(ctx.buf, ctx.area, ctx.design, Some(title), true);
        let width = inner.width;
        if inner.height < 3 || width < 4 {
            return;
        }
        let mut y = inner.y;
        let question = format!("\u{203a} {}", self.question);
        put_trunc(ctx.buf, inner.x, y, &question, width, ctx.design.style(TextStyle::Dim));
        y += 2;
        if failed {
            let headline = match self.state() {
                Some(TaskState::Stopped) => "Stopped before it answered",
                _ => "No answer",
            };
            put_trunc(ctx.buf, inner.x, y, headline, width, ctx.design.style(TextStyle::Danger));
            put_trunc(
                ctx.buf,
                inner.x,
                y + 1,
                &self.error(),
                width,
                ctx.design.style(TextStyle::Body),
            );
            return;
        }
        let text = self.text();
        if self.wrapped_for != (width, text.len()) {
            self.wrapped = wrap(&text, width as usize);
            self.wrapped_for = (width, text.len());
            self.scroll = self.scroll.min(self.wrapped.len().saturating_sub(1));
        }
        let bottom = inner.bottom().saturating_sub(u16::from(self.message.is_some()));
        for line in self.wrapped.iter().skip(self.scroll) {
            if y >= bottom {
                break;
            }
            put(ctx.buf, inner.x, y, line, width, ctx.design.style(TextStyle::Body));
            y += 1;
        }
        if let Some(message) = &self.message {
            put_trunc(
                ctx.buf,
                inner.x,
                inner.bottom().saturating_sub(1),
                message,
                width,
                ctx.design
                    .style(TextStyle::Body)
                    .fg(ctx.design.color(ColorRole::Ok)),
            );
        }
    }
}

/// Wrap `text` to `width` cells at spaces, keeping its own line breaks; a word
/// longer than the line is cut, never left to run off the card.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut line_width = 0usize;
        for word in paragraph.split(' ') {
            let mut word = word.to_string();
            let mut word_width = UnicodeWidthStr::width(word.as_str());
            if line_width > 0 && line_width + 1 + word_width > width {
                out.push(std::mem::take(&mut line));
                line_width = 0;
            }
            while word_width > width {
                // Cut at the last character that fits.
                let mut cut = 0;
                let mut cells = 0;
                for (index, ch) in word.char_indices() {
                    let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]) as &str);
                    if cells + w > width {
                        break;
                    }
                    cells += w;
                    cut = index + ch.len_utf8();
                }
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                out.push(word[..cut].to_string());
                word = word[cut..].to_string();
                word_width = UnicodeWidthStr::width(word.as_str());
            }
            if line_width > 0 {
                line.push(' ');
                line_width += 1;
            }
            line.push_str(&word);
            line_width += word_width;
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_at_spaces_and_keeps_paragraphs() {
        assert_eq!(
            wrap("one two three four\n\nfive", 9),
            vec!["one two", "three", "four", "", "five"]
        );
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        for line in wrap(&"word ".repeat(200), 37) {
            assert!(UnicodeWidthStr::width(line.as_str()) <= 37, "{line:?}");
        }
    }
}
