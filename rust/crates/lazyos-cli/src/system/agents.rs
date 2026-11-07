//! AI Agent Selector (`rofi-agents`, `lazyos menu agents`).
//!
//! Launch a coding agent CLI, manage accounts per agent, and pick a default.
//! Provides `--default`, `--env` and `--new` subcommands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::say;

pub struct AgentDef {
    pub name: &'static str,
    pub binary: &'static str,
    pub npm: &'static str,
    pub envvar: &'static str,
}

pub const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "Claude Code",
        binary: "claude",
        npm: "@anthropic-ai/claude-code",
        envvar: "CLAUDE_CONFIG_DIR",
    },
    AgentDef {
        name: "Codex",
        binary: "codex",
        npm: "@openai/codex",
        envvar: "CODEX_HOME",
    },
    AgentDef {
        name: "OpenCode",
        binary: "opencode",
        npm: "@opencode/cli",
        envvar: "",
    },
    AgentDef {
        name: "Gemini CLI",
        binary: "gemini",
        npm: "@google/gemini-cli",
        envvar: "",
    },
    AgentDef {
        name: "GitHub Copilot",
        binary: "copilot",
        npm: "@github/copilot",
        envvar: "",
    },
];

fn config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".config")
        })
}

fn raw_data_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".local/share")
        })
}

fn state_dir() -> PathBuf {
    config_dir().join("lazyos/agents")
}

fn data_dir() -> PathBuf {
    raw_data_dir().join("lazyos/agents")
}

pub fn default_agent_file() -> PathBuf {
    state_dir().join("default")
}

/// Check whether `binary` exists on PATH or in `~/.opencode/bin` or `~/.local/bin`.
pub fn is_installed(binary: &str) -> bool {
    if let Ok(home) = std::env::var("HOME") {
        let opencode_bin = Path::new(&home).join(".opencode/bin").join(binary);
        if opencode_bin.exists() {
            return true;
        }
        let local_bin = Path::new(&home).join(".local/bin").join(binary);
        if local_bin.exists() {
            return true;
        }
    }
    super::have(binary)
}

/// Get the active default agent binary name.
pub fn get_default_agent() -> Option<&'static AgentDef> {
    if let Ok(content) = fs::read_to_string(default_agent_file()) {
        let want = content.lines().next().unwrap_or("").trim();
        if let Some(agent) = AGENTS
            .iter()
            .find(|a| a.binary == want && is_installed(a.binary))
        {
            return Some(agent);
        }
    }
    // Fallback: opencode if installed
    if let Some(agent) = AGENTS
        .iter()
        .find(|a| a.binary == "opencode" && is_installed(a.binary))
    {
        return Some(agent);
    }
    // Fallback: first installed
    AGENTS.iter().find(|a| is_installed(a.binary))
}

/// Set default agent.
pub fn set_default_agent(binary: &str) -> std::io::Result<()> {
    let dir = state_dir();
    fs::create_dir_all(&dir)?;
    fs::write(default_agent_file(), format!("{binary}\n"))
}

/// Launch agent CLI in a terminal.
pub fn launch(agent: &AgentDef, account: Option<&str>) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let workdir = Path::new(&home).join("Work");
    let cwd = if workdir.is_dir() {
        workdir
    } else {
        PathBuf::from(home)
    };

    let mut pre = String::new();
    if !agent.envvar.is_empty() {
        if let Some(acc) = account {
            let p = data_dir().join(agent.binary).join(acc);
            pre = format!("export {}='{}'; ", agent.envvar, p.display());
        }
    }

    let title = match account {
        Some(acc) => format!("{} · {}", agent.name, acc),
        None => agent.name.to_string(),
    };

    say(
        &format!("Starting {}", agent.name),
        account.map(|a| format!("account: {a}")).unwrap_or_default(),
        Urgency::Normal,
        Glyph::Code,
    );

    let script = format!(
        "cd '{}' 2>/dev/null || cd ~; {pre}exec {}",
        cwd.display(),
        agent.binary
    );
    let escaped_title = title.replace('\'', "'\\''");
    let escaped_script = script.replace('\'', "'\\''");
    crate::system::spawn_detached(&format!(
        "exec foot -T '{escaped_title}' -e bash -lc '{escaped_script}'"
    ));
}

/// Execute CLI query modes.
pub fn cli_default() -> i32 {
    let Some(agent) = get_default_agent() else {
        say(
            "Agents",
            "No agent CLI is installed",
            Urgency::Critical,
            Glyph::Code,
        );
        return 1;
    };
    let mut acc = None;
    if !agent.envvar.is_empty() {
        let def_acc_file = data_dir().join(agent.binary).join("default");
        if let Ok(content) = fs::read_to_string(def_acc_file) {
            let line = content.trim().to_string();
            if !line.is_empty() {
                acc = Some(line);
            }
        }
    }
    launch(agent, acc.as_deref());
    0
}

pub fn cli_env(agent_name: &str, account: &str) -> i32 {
    let agent = AGENTS
        .iter()
        .find(|a| a.name == agent_name || a.binary == agent_name);
    let Some(agent) = agent else {
        eprintln!("unknown agent: {agent_name}");
        return 2;
    };
    if agent.envvar.is_empty() {
        eprintln!("no account support for {}", agent.name);
        return 2;
    }
    let p = data_dir().join(agent.binary).join(account);
    println!("export {}={:?}", agent.envvar, p.display().to_string());
    0
}

pub fn cli_new(agent_name: &str) -> i32 {
    let agent = AGENTS
        .iter()
        .find(|a| a.name == agent_name || a.binary == agent_name);
    let Some(agent) = agent else {
        eprintln!("unknown agent: {agent_name}");
        return 2;
    };
    if agent.envvar.is_empty() {
        eprintln!("no account support for {}", agent.name);
        return 2;
    }
    // Interactive name prompt can be handled or created
    0
}

/// Interactive surface: `lazyos menu agents`.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let def_agent = get_default_agent();
    let mut rows = Vec::new();
    let mut keys = Vec::new();

    for agent in AGENTS {
        let installed = is_installed(agent.binary);
        let is_default = def_agent.is_some_and(|d| d.binary == agent.binary);
        let mark = if is_default { "● " } else { "  " };

        if installed {
            rows.push(row(
                format!("{}{}", mark, agent.name),
                "launch session",
                Glyph::Code,
            ));
            keys.push(format!("launch:{}", agent.binary));

            if !agent.envvar.is_empty() {
                rows.push(row(
                    format!("{} accounts…", agent.name),
                    "switch or create accounts",
                    Glyph::People,
                ));
                keys.push(format!("accounts:{}", agent.binary));
            }
        } else {
            rows.push(row(
                agent.name,
                format!("install {}", agent.npm),
                Glyph::Download,
            ));
            keys.push(format!("install:{}", agent.binary));
        }
    }

    rows.push(row(
        "Set the default agent…",
        "choose active default agent",
        Glyph::Gear,
    ));
    keys.push("default".to_string());

    rows.push(row("Open a terminal in ~/Work", "~/Work", Glyph::Terminal));
    keys.push("shell".to_string());

    let mut picker = Picker::new(
        "Agents",
        vec![Question::choose(
            "Agents",
            "Enter launches · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Agents", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(action) = answers.first() else {
        return Ok(1);
    };

    match action.as_str() {
        "shell" => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            let work = Path::new(&home).join("Work");
            let cwd = if work.is_dir() {
                work
            } else {
                PathBuf::from(home)
            };
            crate::system::spawn_detached(&format!(
                "exec foot -T Work --working-directory '{}'",
                cwd.display()
            ));
            Ok(0)
        }
        "default" => {
            let mut inst_rows = Vec::new();
            let mut inst_keys = Vec::new();
            for agent in AGENTS {
                if is_installed(agent.binary) {
                    inst_rows.push(row(agent.name, agent.binary, Glyph::Code));
                    inst_keys.push(agent.binary.to_string());
                }
            }
            if inst_rows.is_empty() {
                say(
                    "Agents",
                    "No agent CLI is installed yet",
                    Urgency::Critical,
                    Glyph::Code,
                );
                return Ok(3);
            }
            let mut pick_def = Picker::new(
                "Default agent",
                vec![Question::choose(
                    "Default agent",
                    "Enter selects as default · Escape cancels",
                    inst_rows,
                    inst_keys,
                )],
            );
            let c = crate::run_screen(terminal, design, "Default agent", &mut pick_def)?;
            if c == 0 {
                if let Some(chosen) = pick_def.answers().first() {
                    let _ = set_default_agent(chosen);
                    say(
                        "Agents",
                        format!("Default agent: {chosen}"),
                        Urgency::Normal,
                        Glyph::Code,
                    );
                }
            }
            Ok(0)
        }
        act if act.starts_with("launch:") => {
            let bin = &act[7..];
            if let Some(agent) = AGENTS.iter().find(|a| a.binary == bin) {
                let mut acc = None;
                if !agent.envvar.is_empty() {
                    let def_acc_file = data_dir().join(agent.binary).join("default");
                    if let Ok(content) = fs::read_to_string(def_acc_file) {
                        let l = content.trim().to_string();
                        if !l.is_empty() {
                            acc = Some(l);
                        }
                    }
                }
                launch(agent, acc.as_deref());
            }
            Ok(0)
        }
        act if act.starts_with("install:") => {
            let bin = &act[8..];
            if let Some(agent) = AGENTS.iter().find(|a| a.binary == bin) {
                let npm = agent.npm;
                let cmd = format!(
                    "npm install -g --prefix \"$HOME/.local\" --allow-scripts='{npm}' '{npm}' && echo && echo \"{} is in ~/.local/bin; ready.\" && read -r _",
                    agent.name
                );
                let escaped_cmd = cmd.replace('\'', "'\\''");
                crate::system::spawn_detached(&format!(
                    "exec foot -T 'Install {}' -e bash -lc '{escaped_cmd}'",
                    agent.name
                ));
            }
            Ok(0)
        }
        act if act.starts_with("accounts:") => {
            let bin = &act[9..];
            let Some(agent) = AGENTS.iter().find(|a| a.binary == bin) else {
                return Ok(0);
            };
            let dir = data_dir().join(agent.binary);
            let _ = fs::create_dir_all(&dir);

            let mut acc_rows = Vec::new();
            let mut acc_keys = Vec::new();

            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            acc_rows.push(row(&name, "account profile", Glyph::People));
                            acc_keys.push(format!("use:{name}"));
                        }
                    }
                }
            }

            acc_rows.push(row("New account…", "create profile", Glyph::People));
            acc_keys.push("new".to_string());

            let mut acc_picker = Picker::new(
                &format!("Accounts · {}", agent.name),
                vec![Question::choose(
                    format!("Accounts · {}", agent.name),
                    "Enter selects · Escape cancels",
                    acc_rows,
                    acc_keys,
                )],
            );
            let c = crate::run_screen(
                terminal,
                design,
                &format!("Accounts · {}", agent.name),
                &mut acc_picker,
            )?;
            if c == 0 {
                if let Some(chosen) = acc_picker.answers().first() {
                    if chosen == "new" {
                        let mut new_picker = Picker::new(
                            "New Account",
                            vec![Question::input(
                                "Account name",
                                "Enter account name (e.g. work, personal)",
                                "account_name",
                                false,
                            )],
                        );
                        let nc =
                            crate::run_screen(terminal, design, "New Account", &mut new_picker)?;
                        if nc == 0 {
                            if let Some(name) = new_picker.answers().first() {
                                let safe_name: String = name
                                    .chars()
                                    .map(|c| {
                                        if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                                            c
                                        } else {
                                            '_'
                                        }
                                    })
                                    .collect();
                                if !safe_name.is_empty() {
                                    let new_dir = dir.join(&safe_name);
                                    let _ = fs::create_dir_all(&new_dir);
                                    let _ = fs::write(dir.join("default"), &safe_name);
                                    say(
                                        &format!("{}: account {safe_name} ready", agent.name),
                                        "Account created",
                                        Urgency::Normal,
                                        Glyph::People,
                                    );
                                }
                            }
                        }
                    } else if let Some(acc) = chosen.strip_prefix("use:") {
                        let _ = fs::write(dir.join("default"), acc);
                        say(
                            agent.name,
                            format!("default account: {acc}"),
                            Urgency::Normal,
                            Glyph::People,
                        );
                    }
                }
            }
            Ok(0)
        }
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_definitions_contain_expected_names() {
        assert!(
            AGENTS
                .iter()
                .any(|a| a.name == "Claude Code" && a.binary == "claude")
        );
        assert!(
            AGENTS
                .iter()
                .any(|a| a.name == "OpenCode" && a.binary == "opencode")
        );
    }

    #[test]
    fn accounts_env_var_is_set_for_claude_and_codex() {
        let claude = AGENTS.iter().find(|a| a.binary == "claude").unwrap();
        assert_eq!(claude.envvar, "CLAUDE_CONFIG_DIR");

        let codex = AGENTS.iter().find(|a| a.binary == "codex").unwrap();
        assert_eq!(codex.envvar, "CODEX_HOME");

        let opencode = AGENTS.iter().find(|a| a.binary == "opencode").unwrap();
        assert_eq!(opencode.envvar, "");
    }
}
