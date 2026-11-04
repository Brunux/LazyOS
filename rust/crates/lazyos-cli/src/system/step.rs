//! The steps a destructive picker runs, in order, each asking for its own
//! status.
//!
//! The shell learned this the hard way: with `set -e`, a command that fails
//! inside the body of an `if` ends the script before the reason can be
//! written into the card, so the bar spun on a disk that was already broken.
//! Here a step is a value, [`run`] stops at the first failure and answers
//! with that step's name and the reason the tool gave, and the caller turns
//! that into one card.
//!
//! A step is an argv array, so a device name, a label or a time zone read from
//! a person is one argument and never a word a shell would split. `root` is
//! the step's own statement that it needs `sudo -n`: this image has
//! passwordless sudo by design, so `-n` turns a missing permission into a
//! message instead of a prompt with nowhere to appear.

use super::{checked, sudo};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The word the failure names ("partition-table", "mkswap").
    pub name: &'static str,
    pub argv: Vec<String>,
    pub root: bool,
}

impl Step {
    pub fn new(name: &'static str, argv: &[&str]) -> Self {
        Self {
            name,
            argv: argv.iter().map(|word| word.to_string()).collect(),
            root: false,
        }
    }

    /// The same, through `sudo -n`.
    pub fn root(name: &'static str, argv: &[&str]) -> Self {
        Self {
            root: true,
            ..Self::new(name, argv)
        }
    }

    fn run(&self) -> Result<String, String> {
        let argv: Vec<&str> = self.argv.iter().map(String::as_str).collect();
        if self.root {
            sudo(&argv)
        } else {
            checked(&argv)
        }
    }
}

/// A step that failed, and the line its tool printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub step: &'static str,
    pub reason: String,
}

impl Failed {
    /// `the partition step failed: <reason>`: the sentence the card shows.
    pub fn said(&self) -> String {
        format!("the {} step failed: {}", self.step, self.reason)
    }
}

impl From<String> for Failed {
    fn from(reason: String) -> Self {
        Failed {
            step: "unknown",
            reason,
        }
    }
}

impl From<&str> for Failed {
    fn from(reason: &str) -> Self {
        Failed {
            step: "unknown",
            reason: reason.to_string(),
        }
    }
}

/// Run the steps in order and stop at the first failure.
pub fn run(steps: &[Step]) -> Result<(), Failed> {
    for step in steps {
        if let Err(reason) = step.run() {
            return Err(Failed {
                step: step.name,
                reason,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_runs_in_order_and_the_last_one_answers() {
        let steps = [
            Step::new("first", &["sh", "-c", "exit 0"]),
            Step::new("second", &["sh", "-c", "printf 'ok\\n'"]),
        ];
        assert_eq!(run(&steps), Ok(()));
    }

    #[test]
    fn the_failure_names_the_step_that_failed_not_the_first_one() {
        let steps = [
            Step::new("partition-table", &["sh", "-c", "exit 0"]),
            Step::new(
                "filesystem",
                &["sh", "-c", "printf 'mkfs: nope\\n' >&2; exit 1"],
            ),
            Step::new("never", &["sh", "-c", "exit 0"]),
        ];
        let failed = run(&steps).unwrap_err();
        assert_eq!(failed.step, "filesystem");
        assert_eq!(failed.reason, "mkfs: nope");
        assert_eq!(failed.said(), "the filesystem step failed: mkfs: nope");
    }

    #[test]
    fn a_step_is_an_argv_and_never_a_shell_line() {
        let step = Step::new(
            "mkswap",
            &["mkswap", "-f", "-L", "lazyos_swap", "/dev/sda1"],
        );
        assert_eq!(step.argv[3], "lazyos_swap");
        assert!(!step.root);
        assert!(Step::root("parted", &["parted", "-s"]).root);
    }
}
