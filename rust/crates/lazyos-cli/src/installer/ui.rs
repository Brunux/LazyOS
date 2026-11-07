//! Full-screen interactive wizard for LazyOS installation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lazyos_ui::app::{BodyCtx, Outcome, Screen};
use lazyos_ui::design::{ColorRole, Design, TextStyle};
use lazyos_ui::widgets::{
    Input, KeyHint, ListAction, ListState, Row, draw_card, draw_gauge, draw_keybar, put, put_trunc,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

fn fill_rect(buf: &mut ratatui::buffer::Buffer, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}

use super::config::{
    InstallConfig, validate_confirmation, validate_hostname, validate_password, validate_username,
};
use super::preflight::Preflight;
use super::worker::{PHASES, WorkerState, load_state, run_worker, state_file_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardStep {
    Welcome = 0,
    Localization = 1,
    UserAccount = 2,
    DiskReview = 3,
    Progress = 4,
    Complete = 5,
}

impl WizardStep {
    pub fn title(self) -> &'static str {
        match self {
            WizardStep::Welcome => "Pre-Flight & System Overview",
            WizardStep::Localization => "Localization, Timezone & Keyboard",
            WizardStep::UserAccount => "User Account & Host Identity",
            WizardStep::DiskReview => "Target Storage & Partition Review",
            WizardStep::Progress => "Installation in Progress",
            WizardStep::Complete => "Installation Complete",
        }
    }
}

pub struct InstallerScreen {
    pub step: WizardStep,
    pub preflight: Preflight,
    pub config: InstallConfig,
    // Step 1: Localization
    pub tz_input: Input,
    pub tz_list: ListState,
    pub selected_tz: String,
    pub locales: Vec<&'static str>,
    pub locale_idx: usize,
    pub keymaps: Vec<&'static str>,
    pub keymap_idx: usize,
    pub keymap_test: Input,
    pub loc_focus: usize, // 0: tz_input, 1: tz_list, 2: locale, 3: keymap, 4: test, 5: back, 6: next
    // Step 2: User Account
    pub fullname_input: Input,
    pub username_input: Input,
    pub password_input: Input,
    pub confirm_input: Input,
    pub hostname_input: Input,
    pub regen_ssh: bool,
    pub user_focus: usize, // 0: fullname, 1: username, 2: pass, 3: confirm, 4: host, 5: ssh, 6: back, 7: next
    pub user_error: Option<&'static str>,
    // Step 3: Disk Review & Gated Confirmation
    pub disk_confirm_input: Input,
    pub disk_focus: usize, // 0: Cancel/Back, 1: Confirm Input, 2: Erase & Install
    // Step 4: Progress
    pub worker_state: WorkerState,
    pub worker_running: bool,
    pub cancel_worker: Option<Arc<AtomicBool>>,
    pub spinner_idx: usize,
    // Step 5: Complete
    pub complete_focus: usize, // 0: Reboot, 1: Exit Live
    // Keybar cache
    keybar_hints: Vec<KeyHint>,
}

const COMMON_TIMEZONES: &[&str] = &[
    "UTC",
    "America/New_York",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "America/Toronto",
    "America/Mexico_City",
    "America/Bogota",
    "America/Lima",
    "America/Santiago",
    "America/Sao_Paulo",
    "America/Buenos_Aires",
    "Europe/London",
    "Europe/Madrid",
    "Europe/Paris",
    "Europe/Berlin",
    "Europe/Rome",
    "Europe/Amsterdam",
    "Europe/Warsaw",
    "Europe/Kyiv",
    "Europe/Athens",
    "Asia/Tokyo",
    "Asia/Shanghai",
    "Asia/Hong_Kong",
    "Asia/Singapore",
    "Asia/Kolkata",
    "Asia/Dubai",
    "Australia/Sydney",
    "Australia/Melbourne",
    "Pacific/Auckland",
];

const LOCALES: &[&str] = &[
    "en_US.UTF-8",
    "es_ES.UTF-8",
    "fr_FR.UTF-8",
    "de_DE.UTF-8",
    "pt_BR.UTF-8",
    "it_IT.UTF-8",
    "ja_JP.UTF-8",
    "zh_CN.UTF-8",
];

const KEYMAPS: &[&str] = &["us", "es", "latam", "gb", "de", "fr", "it", "pt"];

impl InstallerScreen {
    pub fn new(preflight: Preflight, dry_run: bool) -> Self {
        let config = InstallConfig {
            dry_run,
            ..Default::default()
        };

        let tz_rows: Vec<Row> = COMMON_TIMEZONES
            .iter()
            .map(|&tz| Row::new(tz).detail("IANA timezone"))
            .collect();
        let mut tz_list = ListState::new();
        tz_list.set_rows(tz_rows);

        let mut screen = Self {
            step: WizardStep::Welcome,
            preflight,
            config,
            tz_input: Input::new(),
            tz_list,
            selected_tz: "UTC".to_string(),
            locales: LOCALES.to_vec(),
            locale_idx: 0,
            keymaps: KEYMAPS.to_vec(),
            keymap_idx: 0,
            keymap_test: Input::new(),
            loc_focus: 0,
            fullname_input: Input::with_text("Bruno"),
            username_input: Input::with_text("brunux"),
            password_input: Input::new().with_masked(true),
            confirm_input: Input::new().with_masked(true),
            hostname_input: Input::with_text("lazyos"),
            regen_ssh: true,
            user_focus: 0,
            user_error: None,
            disk_confirm_input: Input::new(),
            disk_focus: 0, // Starts on Cancel!
            worker_state: WorkerState::default(),
            worker_running: false,
            cancel_worker: None,
            spinner_idx: 0,
            complete_focus: 0,
            keybar_hints: Vec::new(),
        };
        screen.update_keybar();
        screen
    }

    fn update_keybar(&mut self) {
        self.keybar_hints = match self.step {
            WizardStep::Welcome => {
                if self.preflight.is_safe {
                    vec![
                        KeyHint::new("Enter", "continue"),
                        KeyHint::new("Esc", "exit"),
                    ]
                } else {
                    vec![KeyHint::new("Esc / Enter", "exit installer")]
                }
            }
            WizardStep::Localization => vec![
                KeyHint::new("Tab/Shift+Tab", "navigate"),
                KeyHint::new("Enter", "select/next"),
                KeyHint::new("Esc", "back"),
            ],
            WizardStep::UserAccount => vec![
                KeyHint::new("Tab/Shift+Tab", "next field"),
                KeyHint::new("Space", "toggle ssh"),
                KeyHint::new("Enter", "continue"),
                KeyHint::new("Esc", "back"),
            ],
            WizardStep::DiskReview => vec![
                KeyHint::new("Tab/Shift+Tab", "navigate"),
                KeyHint::new("Esc", "back"),
                KeyHint::new("Enter", "confirm/cancel"),
            ],
            WizardStep::Progress => vec![
                KeyHint::new("l", "toggle log"),
                KeyHint::new("Esc", "cancel install"),
            ],
            WizardStep::Complete => vec![
                KeyHint::new("Tab", "switch action"),
                KeyHint::new("Enter", "choose action"),
            ],
        };
    }

    pub fn start_worker(&mut self) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_worker = Some(cancel.clone());
        self.worker_running = true;

        let conf = self.config.clone();
        let _ = std::fs::remove_file(state_file_path());

        std::thread::spawn(move || {
            let _ = run_worker(conf, Some(cancel));
        });
    }
}

impl Screen for InstallerScreen {
    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar_hints)
    }

    fn needs_tick(&self) -> Option<Duration> {
        if self.step == WizardStep::Progress {
            Some(Duration::from_millis(100))
        } else {
            None
        }
    }

    fn tick(&mut self) -> Outcome {
        if self.step == WizardStep::Progress {
            self.spinner_idx = (self.spinner_idx + 1) % 4;
            if let Some(state) = load_state() {
                self.worker_state = state;
                if self.worker_state.status == "success" {
                    self.step = WizardStep::Complete;
                    self.update_keybar();
                }
            }
        }
        Outcome::Continue
    }

    fn key(&mut self, key: KeyEvent, _design: &Design) -> Outcome {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return Outcome::Continue;
        }

        match self.step {
            WizardStep::Welcome => match key.code {
                KeyCode::Esc => Outcome::Done(lazyos_ui::app::exit::ESCAPE),
                KeyCode::Enter => {
                    if self.preflight.is_safe {
                        self.step = WizardStep::Localization;
                        self.update_keybar();
                        Outcome::Consumed
                    } else {
                        Outcome::Done(lazyos_ui::app::exit::ESCAPE)
                    }
                }
                _ => Outcome::Continue,
            },

            WizardStep::Localization => {
                match key.code {
                    KeyCode::Esc => {
                        self.step = WizardStep::Welcome;
                        self.update_keybar();
                        return Outcome::Consumed;
                    }
                    KeyCode::Tab => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        if shift {
                            self.loc_focus = if self.loc_focus == 0 {
                                6
                            } else {
                                self.loc_focus - 1
                            };
                        } else {
                            self.loc_focus = (self.loc_focus + 1) % 7;
                        }
                        return Outcome::Consumed;
                    }
                    KeyCode::BackTab => {
                        self.loc_focus = if self.loc_focus == 0 {
                            6
                        } else {
                            self.loc_focus - 1
                        };
                        return Outcome::Consumed;
                    }
                    _ => {}
                }

                match self.loc_focus {
                    0 => {
                        // Timezone search input
                        if key.code == KeyCode::Down {
                            self.loc_focus = 1;
                            return Outcome::Consumed;
                        }
                        self.tz_input.handle(key);
                        self.tz_list.set_filter(self.tz_input.text());
                        Outcome::Consumed
                    }
                    1 => {
                        // Timezone list
                        match self.tz_list.handle(key, 15, _design) {
                            ListAction::Activated(_) => {
                                if let Some(row) = self.tz_list.selected() {
                                    self.selected_tz = row.label.clone();
                                    self.config.timezone = row.label.clone();
                                }
                                Outcome::Consumed
                            }
                            ListAction::Ignored if key.code == KeyCode::Esc => {
                                self.loc_focus = 0;
                                Outcome::Consumed
                            }
                            _ => Outcome::Consumed,
                        }
                    }
                    2 => {
                        // Locale selector
                        match key.code {
                            KeyCode::Left | KeyCode::Up => {
                                self.locale_idx = self.locale_idx.saturating_sub(1);
                                self.config.locale = self.locales[self.locale_idx].to_string();
                            }
                            KeyCode::Right | KeyCode::Down => {
                                self.locale_idx =
                                    (self.locale_idx + 1).min(self.locales.len().saturating_sub(1));
                                self.config.locale = self.locales[self.locale_idx].to_string();
                            }
                            _ => {}
                        }
                        Outcome::Consumed
                    }
                    3 => {
                        // Keymap selector
                        match key.code {
                            KeyCode::Left | KeyCode::Up => {
                                self.keymap_idx = self.keymap_idx.saturating_sub(1);
                                self.config.keymap = self.keymaps[self.keymap_idx].to_string();
                            }
                            KeyCode::Right | KeyCode::Down => {
                                self.keymap_idx =
                                    (self.keymap_idx + 1).min(self.keymaps.len().saturating_sub(1));
                                self.config.keymap = self.keymaps[self.keymap_idx].to_string();
                            }
                            _ => {}
                        }
                        Outcome::Consumed
                    }
                    4 => {
                        // Keymap test box
                        self.keymap_test.handle(key);
                        Outcome::Consumed
                    }
                    5 => {
                        // [ < Back ]
                        if key.code == KeyCode::Enter {
                            self.step = WizardStep::Welcome;
                            self.update_keybar();
                        }
                        Outcome::Consumed
                    }
                    6 => {
                        // [ Next > ]
                        if key.code == KeyCode::Enter {
                            self.config.timezone = self.selected_tz.clone();
                            self.config.locale = self.locales[self.locale_idx].to_string();
                            self.config.keymap = self.keymaps[self.keymap_idx].to_string();
                            self.step = WizardStep::UserAccount;
                            self.update_keybar();
                        }
                        Outcome::Consumed
                    }
                    _ => Outcome::Continue,
                }
            }

            WizardStep::UserAccount => {
                match key.code {
                    KeyCode::Esc => {
                        self.step = WizardStep::Localization;
                        self.update_keybar();
                        return Outcome::Consumed;
                    }
                    KeyCode::Tab => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        if shift {
                            self.user_focus = if self.user_focus == 0 {
                                7
                            } else {
                                self.user_focus - 1
                            };
                        } else {
                            self.user_focus = (self.user_focus + 1) % 8;
                        }
                        return Outcome::Consumed;
                    }
                    KeyCode::BackTab => {
                        self.user_focus = if self.user_focus == 0 {
                            7
                        } else {
                            self.user_focus - 1
                        };
                        return Outcome::Consumed;
                    }
                    _ => {}
                }

                match self.user_focus {
                    0 => {
                        self.fullname_input.handle(key);
                        self.config.fullname = self.fullname_input.text().to_string();
                        Outcome::Consumed
                    }
                    1 => {
                        self.username_input.handle(key);
                        self.config.username = self.username_input.text().to_string();
                        self.user_error = validate_username(&self.config.username).err();
                        Outcome::Consumed
                    }
                    2 => {
                        self.password_input.handle(key);
                        self.config.password = self.password_input.text().to_string();
                        self.user_error =
                            validate_password(&self.config.password, &self.config.password_confirm)
                                .err();
                        Outcome::Consumed
                    }
                    3 => {
                        self.confirm_input.handle(key);
                        self.config.password_confirm = self.confirm_input.text().to_string();
                        self.user_error =
                            validate_password(&self.config.password, &self.config.password_confirm)
                                .err();
                        Outcome::Consumed
                    }
                    4 => {
                        self.hostname_input.handle(key);
                        self.config.hostname = self.hostname_input.text().to_string();
                        self.user_error = validate_hostname(&self.config.hostname).err();
                        Outcome::Consumed
                    }
                    5 => {
                        if key.code == KeyCode::Char(' ') || key.code == KeyCode::Enter {
                            self.regen_ssh = !self.regen_ssh;
                            self.config.regenerate_ssh_keys = self.regen_ssh;
                        }
                        Outcome::Consumed
                    }
                    6 => {
                        if key.code == KeyCode::Enter {
                            self.step = WizardStep::Localization;
                            self.update_keybar();
                        }
                        Outcome::Consumed
                    }
                    7 => {
                        if key.code == KeyCode::Enter {
                            self.config.fullname = self.fullname_input.text().to_string();
                            self.config.username = self.username_input.text().to_string();
                            self.config.password = self.password_input.text().to_string();
                            self.config.password_confirm = self.confirm_input.text().to_string();
                            self.config.hostname = self.hostname_input.text().to_string();
                            self.config.regenerate_ssh_keys = self.regen_ssh;

                            if let Err(err) = self.config.validate_user_fields() {
                                self.user_error = Some(err);
                                return Outcome::Consumed;
                            }
                            self.user_error = None;
                            self.step = WizardStep::DiskReview;
                            self.disk_focus = 0; // Crucial: Starts on Cancel!
                            self.update_keybar();
                        }
                        Outcome::Consumed
                    }
                    _ => Outcome::Continue,
                }
            }

            WizardStep::DiskReview => {
                match key.code {
                    KeyCode::Esc => {
                        self.step = WizardStep::UserAccount;
                        self.update_keybar();
                        return Outcome::Consumed;
                    }
                    KeyCode::Tab => {
                        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                        if shift {
                            self.disk_focus = if self.disk_focus == 0 {
                                2
                            } else {
                                self.disk_focus - 1
                            };
                        } else {
                            self.disk_focus = (self.disk_focus + 1) % 3;
                        }
                        return Outcome::Consumed;
                    }
                    KeyCode::BackTab => {
                        self.disk_focus = if self.disk_focus == 0 {
                            2
                        } else {
                            self.disk_focus - 1
                        };
                        return Outcome::Consumed;
                    }
                    _ => {}
                }

                match self.disk_focus {
                    0 => {
                        // [ Cancel / Back ]
                        if key.code == KeyCode::Enter {
                            self.step = WizardStep::UserAccount;
                            self.update_keybar();
                        }
                        Outcome::Consumed
                    }
                    1 => {
                        // Confirmation input
                        self.disk_confirm_input.handle(key);
                        Outcome::Consumed
                    }
                    2 => {
                        // [ Erase Disk & Install ]
                        if key.code == KeyCode::Enter {
                            if validate_confirmation(self.disk_confirm_input.text(), "mmcblk0") {
                                self.step = WizardStep::Progress;
                                self.update_keybar();
                                self.start_worker();
                            }
                        }
                        Outcome::Consumed
                    }
                    _ => Outcome::Continue,
                }
            }

            WizardStep::Progress => match key.code {
                KeyCode::Esc => {
                    if let Some(ref cancel) = self.cancel_worker {
                        cancel.store(true, Ordering::SeqCst);
                    }
                    Outcome::Done(lazyos_ui::app::exit::ESCAPE)
                }
                _ => Outcome::Consumed,
            },

            WizardStep::Complete => match key.code {
                KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                    self.complete_focus = 1 - self.complete_focus;
                    Outcome::Consumed
                }
                KeyCode::Enter => {
                    if self.complete_focus == 0 {
                        // Reboot System
                        let _ = std::process::Command::new("systemctl")
                            .arg("reboot")
                            .spawn();
                        Outcome::Done(lazyos_ui::app::exit::OK)
                    } else {
                        // Live Session
                        Outcome::Done(lazyos_ui::app::exit::OK)
                    }
                }
                KeyCode::Esc => Outcome::Done(lazyos_ui::app::exit::OK),
                _ => Outcome::Continue,
            },
        }
    }

    fn draw(&mut self, ctx: &mut BodyCtx) {
        let area = ctx.area;
        let design = ctx.design;
        let buf = &mut *ctx.buf;

        if area.width < 50 || area.height < 15 {
            put(
                buf,
                area.x,
                area.y,
                "Terminal too small for installer wizard (min 80x24)",
                area.width,
                design.style(TextStyle::Dim),
            );
            return;
        }

        // Wizard Step Indicator Header
        let step_text = format!(
            " Step {} of 6: {} ",
            self.step as usize + 1,
            self.step.title()
        );
        put(
            buf,
            area.x + 2,
            area.y,
            &step_text,
            area.width.saturating_sub(4),
            design
                .style(TextStyle::Key)
                .add_modifier(Modifier::BOLD)
                .fg(design.color(ColorRole::Accent)),
        );

        let content_area = Rect {
            x: area.x + 1,
            y: area.y + 2,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(3),
        };

        match self.step {
            WizardStep::Welcome => self.draw_welcome(content_area, buf, design),
            WizardStep::Localization => self.draw_localization(content_area, buf, design),
            WizardStep::UserAccount => self.draw_user_account(content_area, buf, design),
            WizardStep::DiskReview => self.draw_disk_review(content_area, buf, design),
            WizardStep::Progress => self.draw_progress(content_area, buf, design),
            WizardStep::Complete => self.draw_complete(content_area, buf, design),
        }
    }
}

impl InstallerScreen {
    fn draw_welcome(&self, area: Rect, buf: &mut ratatui::buffer::Buffer, design: &Design) {
        let card = draw_card(
            buf,
            Rect {
                x: area.x + 1,
                y: area.y,
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(1),
            },
            design,
            Some("LazyOS Full-System Installer"),
            true,
        );

        let mut y = card.y + 1;
        put(
            buf,
            card.x + 2,
            y,
            "Welcome to the LazyOS full-system installer for Google Hayato (MT8192).",
            card.width.saturating_sub(4),
            design.style(TextStyle::Body).add_modifier(Modifier::BOLD),
        );
        y += 2;

        // Hardware Summary
        put(
            buf,
            card.x + 2,
            y,
            "System Hardware & Boot Media:",
            card.width.saturating_sub(4),
            design
                .style(TextStyle::Dim)
                .add_modifier(Modifier::UNDERLINED),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            &format!("• Platform SoC:  {}", self.preflight.soc_model),
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            &format!("• Total Memory:  {}", self.preflight.total_ram),
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            &format!("• Power Status:  {}", self.preflight.power_source),
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            &format!("• Running Root:  {}", self.preflight.root_source),
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            &format!(
                "• Target Disk:   {} ({})",
                self.preflight.target_disk, self.preflight.target_size
            ),
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 2;

        if !self.preflight.is_safe {
            // Safety refusal card
            let danger_area = Rect {
                x: card.x + 2,
                y,
                width: card.width.saturating_sub(4),
                height: 5,
            };
            fill_rect(
                buf,
                danger_area,
                Style::default().bg(design.color(ColorRole::SurfaceAlt)),
            );
            put(
                buf,
                danger_area.x + 2,
                danger_area.y + 1,
                "CRITICAL SAFETY REFUSAL: System already running from target eMMC!",
                danger_area.width.saturating_sub(4),
                Style::default()
                    .fg(design.color(ColorRole::Danger))
                    .add_modifier(Modifier::BOLD),
            );
            put(
                buf,
                danger_area.x + 2,
                danger_area.y + 2,
                "Overwriting the active running disk is strictly blocked to prevent data corruption.",
                danger_area.width.saturating_sub(4),
                design.style(TextStyle::Body),
            );
            put(
                buf,
                danger_area.x + 2,
                danger_area.y + 3,
                "Please boot LazyOS from an external microSD card or USB drive to install.",
                danger_area.width.saturating_sub(4),
                design.style(TextStyle::Dim),
            );
        } else {
            // Preflight check pass badge
            let badge_area = Rect {
                x: card.x + 2,
                y,
                width: card.width.saturating_sub(4),
                height: 3,
            };
            fill_rect(
                buf,
                badge_area,
                Style::default().bg(design.color(ColorRole::Surface)),
            );
            put(
                buf,
                badge_area.x + 2,
                badge_area.y + 1,
                "✓ Pre-flight checks passed: internal eMMC ready for clean installation.",
                badge_area.width.saturating_sub(4),
                Style::default()
                    .fg(design.color(ColorRole::Accent))
                    .add_modifier(Modifier::BOLD),
            );
            y += 4;

            // Required tools list
            put(
                buf,
                card.x + 2,
                y,
                "Verified Installer Utilities:",
                card.width.saturating_sub(4),
                design
                    .style(TextStyle::Dim)
                    .add_modifier(Modifier::UNDERLINED),
            );
            y += 1;
            let mut col1_y = y;
            let mut col2_y = y;
            for (idx, tool) in self.preflight.tools.iter().enumerate() {
                let status_icon = if tool.available { "✓" } else { "✗" };
                let text = format!("{status_icon} {:<12} ({})", tool.name, tool.description);
                if idx % 2 == 0 {
                    put(
                        buf,
                        card.x + 4,
                        col1_y,
                        &text,
                        (card.width / 2).saturating_sub(4),
                        design.style(TextStyle::Body),
                    );
                    col1_y += 1;
                } else {
                    put(
                        buf,
                        card.x + card.width / 2 + 2,
                        col2_y,
                        &text,
                        (card.width / 2).saturating_sub(4),
                        design.style(TextStyle::Body),
                    );
                    col2_y += 1;
                }
            }
        }
    }

    fn draw_localization(
        &mut self,
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        design: &Design,
    ) {
        // Left Column: Timezone (Search + List)
        let left_w = area.width / 2;
        let right_w = area.width.saturating_sub(left_w);

        let tz_card = draw_card(
            buf,
            Rect {
                x: area.x,
                y: area.y,
                width: left_w.saturating_sub(1),
                height: area.height,
            },
            design,
            Some("Timezone Selection"),
            self.loc_focus == 0 || self.loc_focus == 1,
        );

        put(
            buf,
            tz_card.x + 1,
            tz_card.y,
            "Filter Timezones:",
            tz_card.width.saturating_sub(2),
            design.style(TextStyle::Dim),
        );
        let search_area = Rect {
            x: tz_card.x + 1,
            y: tz_card.y + 1,
            width: tz_card.width.saturating_sub(2),
            height: 1,
        };
        self.tz_input.render(
            search_area,
            buf,
            design,
            "Type to filter (e.g. America, Europe)...",
        );

        let list_area = Rect {
            x: tz_card.x + 1,
            y: tz_card.y + 3,
            width: tz_card.width.saturating_sub(2),
            height: tz_card.height.saturating_sub(4),
        };
        self.tz_list
            .render(list_area, buf, design, self.loc_focus == 1);

        // Right Column: Locale, Keyboard, Test Box, and Buttons
        let right_card = draw_card(
            buf,
            Rect {
                x: area.x + left_w,
                y: area.y,
                width: right_w,
                height: area.height,
            },
            design,
            Some("Locale & Keyboard Setup"),
            self.loc_focus >= 2,
        );

        let mut y = right_card.y + 1;
        // Selected Timezone feedback
        put(
            buf,
            right_card.x + 2,
            y,
            &format!("Selected Timezone:  {}", self.selected_tz),
            right_card.width.saturating_sub(4),
            design.style(TextStyle::Body).add_modifier(Modifier::BOLD),
        );
        y += 2;

        // Locale Selector
        let loc_style = if self.loc_focus == 2 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };
        put(
            buf,
            right_card.x + 2,
            y,
            &format!("System Locale:  < {} >", self.locales[self.locale_idx]),
            right_card.width.saturating_sub(4),
            loc_style,
        );
        put(
            buf,
            right_card.x + 2,
            y + 1,
            "(Use Left/Right arrow keys to switch locale)",
            right_card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 3;

        // Keyboard Layout Selector
        let kbd_style = if self.loc_focus == 3 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };
        put(
            buf,
            right_card.x + 2,
            y,
            &format!("Keyboard Layout:  < {} >", self.keymaps[self.keymap_idx]),
            right_card.width.saturating_sub(4),
            kbd_style,
        );
        put(
            buf,
            right_card.x + 2,
            y + 1,
            "(Use Left/Right arrow keys to switch keymap)",
            right_card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 3;

        // Interactive Keypress Test Box
        put(
            buf,
            right_card.x + 2,
            y,
            "Interactive Keyboard Test (Type here to verify symbols):",
            right_card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 1;
        let test_box = Rect {
            x: right_card.x + 2,
            y,
            width: right_card.width.saturating_sub(4),
            height: 1,
        };
        self.keymap_test.render(
            test_box,
            buf,
            design,
            "Type test keys: @ # $ % & / ( ) = ? ¡ ¿ ~",
        );
        y += 3;

        // Navigation Action Buttons
        let back_style = if self.loc_focus == 5 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };
        let next_style = if self.loc_focus == 6 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };

        put(buf, right_card.x + 4, y, "[ < Back ]", 10, back_style);
        put(
            buf,
            right_card.x + 18,
            y,
            "[ Next: User Account > ]",
            24,
            next_style,
        );
    }

    fn draw_user_account(
        &mut self,
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        design: &Design,
    ) {
        let card = draw_card(
            buf,
            Rect {
                x: area.x + area.width / 6,
                y: area.y,
                width: (area.width * 2) / 3,
                height: area.height,
            },
            design,
            Some("Primary User Account & Machine Identity"),
            true,
        );

        let mut y = card.y + 1;
        put(
            buf,
            card.x + 4,
            y,
            "Full Name (Display Name):",
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 1;
        self.fullname_input.render(
            Rect {
                x: card.x + 4,
                y,
                width: card.width.saturating_sub(8),
                height: 1,
            },
            buf,
            design,
            "Your full name (e.g. Bruno)",
        );
        y += 2;

        put(
            buf,
            card.x + 4,
            y,
            "POSIX Username (Login & Sudo User):",
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 1;
        self.username_input.render(
            Rect {
                x: card.x + 4,
                y,
                width: card.width.saturating_sub(8),
                height: 1,
            },
            buf,
            design,
            "lowercase login name (e.g. brunux)",
        );
        y += 2;

        put(
            buf,
            card.x + 4,
            y,
            "Account Password:",
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 1;
        self.password_input.render(
            Rect {
                x: card.x + 4,
                y,
                width: card.width.saturating_sub(8),
                height: 1,
            },
            buf,
            design,
            "enter password",
        );
        y += 2;

        put(
            buf,
            card.x + 4,
            y,
            "Confirm Password:",
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 1;
        self.confirm_input.render(
            Rect {
                x: card.x + 4,
                y,
                width: card.width.saturating_sub(8),
                height: 1,
            },
            buf,
            design,
            "re-type password",
        );
        y += 2;

        put(
            buf,
            card.x + 4,
            y,
            "System Hostname:",
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 1;
        self.hostname_input.render(
            Rect {
                x: card.x + 4,
                y,
                width: card.width.saturating_sub(8),
                height: 1,
            },
            buf,
            design,
            "network hostname (default lazyos)",
        );
        y += 2;

        let check_mark = if self.regen_ssh { "[X]" } else { "[ ]" };
        let ssh_style = if self.user_focus == 5 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };
        put(
            buf,
            card.x + 4,
            y,
            &format!("{check_mark} Generate fresh SSH host keys on first boot"),
            card.width.saturating_sub(8),
            ssh_style,
        );
        y += 2;

        if let Some(err) = self.user_error {
            put(
                buf,
                card.x + 4,
                y,
                &format!("⚠ Validation Error: {err}"),
                card.width.saturating_sub(8),
                Style::default()
                    .fg(design.color(ColorRole::Danger))
                    .add_modifier(Modifier::BOLD),
            );
            y += 2;
        }

        let back_style = if self.user_focus == 6 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };
        let next_style = if self.user_focus == 7 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };

        put(buf, card.x + 4, y, "[ < Back ]", 10, back_style);
        put(
            buf,
            card.x + 20,
            y,
            "[ Next: Review & Confirm > ]",
            28,
            next_style,
        );
    }

    fn draw_disk_review(&mut self, area: Rect, buf: &mut ratatui::buffer::Buffer, design: &Design) {
        let card = draw_card(
            buf,
            Rect {
                x: area.x + 2,
                y: area.y,
                width: area.width.saturating_sub(4),
                height: area.height,
            },
            design,
            Some("Target Storage & Partition Review"),
            true,
        );

        let mut y = card.y + 1;
        put(
            buf,
            card.x + 2,
            y,
            &format!(
                "Target Storage Device: {} ({})",
                self.config.target_disk, self.preflight.target_size
            ),
            card.width.saturating_sub(4),
            design.style(TextStyle::Body).add_modifier(Modifier::BOLD),
        );
        y += 2;

        // Visual Partition Layout Diagram
        put(
            buf,
            card.x + 2,
            y,
            "Target ChromeOS GPT Partition Layout:",
            card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            "┌─────────────────┬───────────────────┬──────────────────────────────────────┐",
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            "│ p1: pmOS_kernel │   p2: pmOS_boot   │             p3: pmOS_root            │",
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            "│     128 MiB     │      512 MiB      │               ~57.6 GiB              │",
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            "└─────────────────┴───────────────────┴──────────────────────────────────────┘",
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;

        put(
            buf,
            card.x + 4,
            y,
            "• p1 (128 MiB): ChromeOS signed FIT kernel partition (Depthcharge bootloader)",
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            "• p2 (512 MiB): Boot filesystem (ext2, mounted to /boot)",
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            "• p3 (~57.6 GiB): Debian arm64 root filesystem (ext4, mounted to /)",
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 2;

        // Eye-candy Danger Alert Box
        let danger_box = Rect {
            x: card.x + 2,
            y,
            width: card.width.saturating_sub(4),
            height: 4,
        };
        fill_rect(
            buf,
            danger_box,
            Style::default().bg(design.color(ColorRole::SurfaceAlt)),
        );
        put(
            buf,
            danger_box.x + 2,
            danger_box.y + 1,
            "⚠ WARNING: ALL EXISTING DATA ON TARGET DRIVE WILL BE PERMANENTLY ERASED!",
            danger_box.width.saturating_sub(4),
            Style::default()
                .fg(design.color(ColorRole::Danger))
                .add_modifier(Modifier::BOLD),
        );
        put(
            buf,
            danger_box.x + 2,
            danger_box.y + 2,
            "This action destroys all ChromeOS or Linux partitions on /dev/mmcblk0.",
            danger_box.width.saturating_sub(4),
            design.style(TextStyle::Body),
        );
        y += 5;

        // Confirmation Gate Input
        let confirmed = validate_confirmation(self.disk_confirm_input.text(), "mmcblk0");
        put(
            buf,
            card.x + 2,
            y,
            "Type \"mmcblk0\" to confirm disk erasure:",
            card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 1;
        let confirm_box = Rect {
            x: card.x + 2,
            y,
            width: 30,
            height: 1,
        };
        self.disk_confirm_input
            .render(confirm_box, buf, design, "type mmcblk0");
        y += 3;

        // Action Buttons: Focus defaults to Cancel!
        let cancel_style = if self.disk_focus == 0 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };

        let erase_style = if !confirmed {
            design.style(TextStyle::Dim)
        } else if self.disk_focus == 2 {
            Style::default()
                .bg(design.color(ColorRole::Danger))
                .fg(design.color(ColorRole::Surface))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(design.color(ColorRole::Danger))
                .add_modifier(Modifier::BOLD)
        };

        put(buf, card.x + 2, y, "[ Cancel / Back ]", 17, cancel_style);
        put(
            buf,
            card.x + 22,
            y,
            "[ Erase Disk & Install LazyOS ]",
            32,
            erase_style,
        );
    }

    fn draw_progress(&mut self, area: Rect, buf: &mut ratatui::buffer::Buffer, design: &Design) {
        let card = draw_card(
            buf,
            Rect {
                x: area.x + 1,
                y: area.y,
                width: area.width.saturating_sub(2),
                height: area.height,
            },
            design,
            Some("Installation Progress"),
            true,
        );

        let mut y = card.y + 1;
        // Overall Gauge Bar
        let pct = self.worker_state.percent as f64 / 100.0;
        let gauge_label = format!(
            "{}% - {}",
            self.worker_state.percent, self.worker_state.phase_name
        );
        draw_gauge(
            buf,
            Rect {
                x: card.x + 2,
                y,
                width: card.width.saturating_sub(4),
                height: 1,
            },
            design,
            pct,
            &gauge_label,
        );
        y += 2;

        // Sequential Phase Status Checklist
        put(
            buf,
            card.x + 2,
            y,
            "Installation Sequence:",
            card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 1;

        let spinners = ["◴", "◷", "◶", "◵"];
        let current_spinner = spinners[self.spinner_idx];

        for (idx, phase) in PHASES.iter().enumerate() {
            let (icon, style) = if idx < self.worker_state.phase_index {
                ("✓", Style::default().fg(design.color(ColorRole::Accent)))
            } else if idx == self.worker_state.phase_index {
                if self.worker_state.status == "failed" {
                    ("✗", Style::default().fg(design.color(ColorRole::Danger)))
                } else if self.worker_state.status == "success" {
                    ("✓", Style::default().fg(design.color(ColorRole::Accent)))
                } else {
                    (current_spinner, design.style(TextStyle::Key))
                }
            } else {
                ("○", design.style(TextStyle::Dim))
            };

            put(
                buf,
                card.x + 4,
                y,
                &format!("[{icon}] {}. {phase}", idx + 1),
                card.width.saturating_sub(8),
                style,
            );
            y += 1;
        }
        y += 1;

        // Streaming Log Pane
        put(
            buf,
            card.x + 2,
            y,
            "Live Installation Log:",
            card.width.saturating_sub(4),
            design.style(TextStyle::Dim),
        );
        y += 1;

        let log_area = Rect {
            x: card.x + 2,
            y,
            width: card.width.saturating_sub(4),
            height: card.bottom().saturating_sub(y + 1),
        };
        fill_rect(
            buf,
            log_area,
            Style::default().bg(design.color(ColorRole::Surface)),
        );

        let tail = if self.worker_state.log_lines.len() > log_area.height as usize {
            &self.worker_state.log_lines
                [self.worker_state.log_lines.len() - log_area.height as usize..]
        } else {
            &self.worker_state.log_lines[..]
        };

        for (line_idx, line) in tail.iter().enumerate() {
            put_trunc(
                buf,
                log_area.x + 1,
                log_area.y + line_idx as u16,
                line,
                log_area.width.saturating_sub(2),
                design.style(TextStyle::Dim),
            );
        }
    }

    fn draw_complete(&mut self, area: Rect, buf: &mut ratatui::buffer::Buffer, design: &Design) {
        let card = draw_card(
            buf,
            Rect {
                x: area.x + area.width / 6,
                y: area.y + 2,
                width: (area.width * 2) / 3,
                height: area.height.saturating_sub(4),
            },
            design,
            Some("Installation Finished"),
            true,
        );

        let mut y = card.y + 2;
        put(
            buf,
            card.x + 4,
            y,
            "✓ LazyOS Successfully Installed!",
            card.width.saturating_sub(8),
            Style::default()
                .fg(design.color(ColorRole::Accent))
                .add_modifier(Modifier::BOLD),
        );
        y += 2;

        put(
            buf,
            card.x + 4,
            y,
            &format!(
                "Target eMMC storage ({}) is now prepared and bootable.",
                self.config.target_disk
            ),
            card.width.saturating_sub(8),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 4,
            y,
            &format!(
                "Primary user: {} · Timezone: {} · Locale: {}",
                self.config.username, self.config.timezone, self.config.locale
            ),
            card.width.saturating_sub(8),
            design.style(TextStyle::Dim),
        );
        y += 3;

        put(
            buf,
            card.x + 4,
            y,
            "How to boot your new installation:",
            card.width.saturating_sub(8),
            design
                .style(TextStyle::Body)
                .add_modifier(Modifier::UNDERLINED),
        );
        y += 1;
        put(
            buf,
            card.x + 6,
            y,
            "1. Reboot your Chromebook.",
            card.width.saturating_sub(12),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 6,
            y,
            "2. At the developer mode boot menu, choose \"internal disk\".",
            card.width.saturating_sub(12),
            design.style(TextStyle::Body),
        );
        y += 1;
        put(
            buf,
            card.x + 6,
            y,
            "3. If the microSD card is removed, the internal eMMC boots automatically.",
            card.width.saturating_sub(12),
            design.style(TextStyle::Body),
        );
        y += 4;

        let reboot_style = if self.complete_focus == 0 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };
        let exit_style = if self.complete_focus == 1 {
            design.style(TextStyle::Key).add_modifier(Modifier::BOLD)
        } else {
            design.style(TextStyle::Body)
        };

        put(
            buf,
            card.x + 6,
            y,
            "[ Reboot System Now ]",
            22,
            reboot_style,
        );
        put(
            buf,
            card.x + 32,
            y,
            "[ Stay in Live Session ]",
            24,
            exit_style,
        );
    }
}
