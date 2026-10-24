//! The task surface: reattach to a slow action and watch it finish.
//!
//! A task is created by the menus that own the action (Wi-Fi scan, apt,
//! dictation, the installer). This command only reopens one by id, so a person
//! who closed the window can come back to the result and the log.

use std::io;

use lazyos_ui::{App, Design, Header, Task, TaskView};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::Failure;
use crate::command::Resolved;
use crate::paths::Paths;

pub fn attach(
    resolved: &Resolved,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
) -> Result<i32, Failure> {
    let id = match resolved.args.first() {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return Err(Failure::new("no task id", "usage: lazyos task attach <id>"));
        }
    };

    let paths = Paths::from_process().map_err(|error| {
        Failure::new("cannot find the session state directory", error.to_string())
    })?;

    let task = match Task::load(&id, &paths.state) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return Err(Failure::new(
                format!("task '{id}' was not found"),
                "the task may have been cleaned up, or the id is wrong",
            ));
        }
        Err(error) => {
            return Err(Failure::new(
                format!("task '{id}' could not be read"),
                error.to_string(),
            ));
        }
    };

    let title = task.title.clone();
    let mut view = TaskView::new(task);
    let mut app = App::new(Header::new("LazyOS").subtitle(&title))
        .design(design.clone())
        .screen(&mut view);
    app.run(terminal)
        .map_err(|error| Failure::new("the terminal stopped answering", error.to_string()))
}
