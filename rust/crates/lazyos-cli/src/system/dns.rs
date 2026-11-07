//! DNS: point the active connection at a resolver, or back at DHCP (`rofi-dns`).
//!
//! NetworkManager keeps /etc/resolv.conf, so the switch is two nmcli
//! properties on the connection that is up — `ipv4.dns` and
//! `ipv4.ignore-auto-dns` — followed by bringing that connection up again
//! so the change is applied at once.
//!
//! That re-activation is why this says something before it acts: a Wi-Fi
//! connection that comes back up costs a second or two, and a menu that is
//! already closed while the network blinks is indistinguishable from a bug.
//! The `busy` card stays until the connection answers again and then becomes
//! the read-back from NetworkManager, not the value that was asked for.
//!
//! The card registers no stop command, and that is a decision, not an
//! omission. Killing `nmcli connection up` is safe in itself (NetworkManager
//! finishes or drops the activation on its own), and it is still the wrong
//! button: by the time the activation runs, `connection modify` has already
//! written the new servers to the profile, so a stop leaves the profile
//! saying Cloudflare while resolv.conf still holds whatever DHCP handed out
//! — a mismatch with nothing on screen to explain it.
//!
//! DNS is not the same as a VPN: the addresses below are public resolvers
//! reached in the clear unless the network itself is encrypted. Nothing here
//! touches IPv6 DNS (`ipv6.dns`), which the same nmcli line could set.

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say, sudo};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["nmcli"];

/// Get the active non-loopback device.
fn active_device() -> Option<String> {
    out(&["nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device", "status"])
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            let parts: Vec<&str> = l.split(':').collect();
            if parts.len() >= 3 && parts[2] == "connected" && parts[1] != "loopback" {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
}

/// Get the active connection name for a device.
fn active_connection(dev: &str) -> Option<String> {
    out(&["nmcli", "-g", "GENERAL.CONNECTION", "device", "show", dev])
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty() && s != "--")
}

/// The `--status` output.
pub fn status() -> String {
    let Some(dev) = active_device() else {
        return "No connection is up".to_string();
    };

    let Some(con) = active_connection(&dev) else {
        return format!("{} has no active connection", dev);
    };

    let current = out(&["nmcli", "-g", "IP4.DNS", "device", "show", &dev]).unwrap_or_default();
    let current = current.lines().collect::<Vec<_>>().join(", ");
    let auto = out(&[
        "nmcli",
        "-g",
        "ipv4.ignore-auto-dns",
        "connection",
        "show",
        &con,
    ])
    .unwrap_or_default();
    let auto = if auto == "no" { "no" } else { "yes" };

    format!(
        "On {} ({}) · now: {} · ignore-auto-dns: {}",
        con,
        dev,
        if current.is_empty() {
            "automatic".to_string()
        } else {
            current
        },
        auto
    )
}

/// `lazyos menu dns` (`rofi-dns`): the DNS picker.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "DNS",
            format!("{said} (NetworkManager)"),
            Urgency::Critical,
            Glyph::Cloud,
        );
        return Ok(3);
    }

    let dev = match active_device() {
        Some(d) => d,
        None => {
            say(
                "DNS",
                "No connection is up, so there is nothing to change".to_string(),
                Urgency::Critical,
                Glyph::Cloud,
            );
            return Ok(3);
        }
    };

    let con = match active_connection(&dev) {
        Some(c) => c,
        None => {
            say(
                "DNS",
                format!("{} has no active connection", dev),
                Urgency::Critical,
                Glyph::Cloud,
            );
            return Ok(3);
        }
    };

    // What is in use right now, and how it got there.
    let current = out(&["nmcli", "-g", "IP4.DNS", "device", "show", &dev]).unwrap_or_default();
    let current = current.lines().collect::<Vec<_>>().join(" ");
    let auto = out(&[
        "nmcli",
        "-g",
        "ipv4.ignore-auto-dns",
        "connection",
        "show",
        &con,
    ])
    .unwrap_or_default();
    let auto = if auto == "no" { "no" } else { "yes" };

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    // Automatic (DHCP)
    if auto == "no" && current.is_empty() {
        rows.push(row(
            "Automatic (the network)",
            "no static servers · in use",
            Glyph::Cloud,
        ));
    } else if auto == "no" && !current.is_empty() {
        rows.push(row(
            "Automatic (the network)",
            format!("{} · in use", current),
            Glyph::Cloud,
        ));
    } else {
        rows.push(row(
            "Automatic (the network)",
            "what DHCP hands out",
            Glyph::Cloud,
        ));
    }
    keys.push("automatic".to_string());

    // Preset resolvers
    let resolvers = [
        ("Cloudflare", "1.1.1.1,1.0.0.1"),
        ("Google", "8.8.8.8,8.8.4.4"),
        ("Quad9", "9.9.9.9,149.112.112.112"),
        ("AdGuard", "94.140.14.14,94.140.15.15"),
    ];
    for (name, servers) in resolvers {
        let servers_space = servers.replace(',', " ");
        let in_use = auto == "no" && current == servers_space;
        if in_use {
            rows.push(row(name, format!("{} · in use", servers), Glyph::Cloud));
        } else {
            rows.push(row(name, servers, Glyph::Cloud));
        }
        keys.push(format!("dns:{}", servers));
    }

    // Custom
    rows.push(row(
        "Type the servers",
        "comma or space separated",
        Glyph::Search,
    ));
    keys.push("custom".to_string());

    let mut picker = Picker::new(
        "DNS",
        vec![Question::choose(
            "DNS",
            format!(
                "On {} ({}) · now: {}",
                con,
                dev,
                if current.is_empty() {
                    "automatic".to_string()
                } else {
                    current
                }
            ),
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "DNS", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    if answers.is_empty() {
        return Ok(1);
    }
    let key = &answers[0];

    let servers = match key.as_str() {
        "automatic" => String::new(),
        k if k.starts_with("dns:") => k[4..].to_string(),
        "custom" => {
            // Use the free-text picker for custom servers
            let input = out(&[
                "sh",
                "-c",
                "lazyos menu --ask 'DNS servers' 'IPv4 addresses, comma or space separated'",
            ])
            .unwrap_or_default();
            let input = input.trim();
            if input.is_empty() {
                return Ok(1);
            }
            input
                .replace([' ', ','], "\n")
                .lines()
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        }
        _ => return Ok(0),
    };

    // Check for root/sudo
    if !sudo_available() {
        say(
            "DNS",
            "Changing a connection needs root and sudo asks for a password here".to_string(),
            Urgency::Critical,
            Glyph::Cloud,
        );
        return Ok(3);
    }

    let label = if servers.is_empty() {
        "Automatic (DHCP)".to_string()
    } else {
        servers.replace(',', " ")
    };
    say(
        "DNS",
        format!("Applying {}", label),
        Urgency::Normal,
        Glyph::Cloud,
    );

    let result = if servers.is_empty() {
        sudo(&[
            "nmcli",
            "connection",
            "modify",
            &con,
            "ipv4.ignore-auto-dns",
            "no",
            "ipv4.dns",
            "",
        ])
        .and_then(|_| sudo(&["nmcli", "connection", "up", &con]))
    } else {
        sudo(&[
            "nmcli",
            "connection",
            "modify",
            &con,
            "ipv4.dns",
            &servers,
            "ipv4.ignore-auto-dns",
            "yes",
        ])
        .and_then(|_| sudo(&["nmcli", "connection", "up", &con]))
    };

    match result {
        Ok(_) => {
            // Read back what NetworkManager is using now
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = out(&["nmcli", "-g", "IP4.DNS", "device", "show", &dev]).unwrap_or_default();
            let now = now.lines().collect::<Vec<_>>().join(" ");
            if now.is_empty() {
                say(
                    "DNS",
                    "Back on the network's servers".to_string(),
                    Urgency::Normal,
                    Glyph::Cloud,
                );
            } else {
                say(
                    "DNS",
                    format!("DNS is now {}", now),
                    Urgency::Normal,
                    Glyph::Cloud,
                );
            }
            Ok(0)
        }
        Err(e) => {
            say(
                "DNS",
                format!("Could not apply: {}", e),
                Urgency::Critical,
                Glyph::Cloud,
            );
            Ok(3)
        }
    }
}

fn missing(names: &[&str]) -> Option<String> {
    let gone: Vec<&str> = names.iter().copied().filter(|n| !have(n)).collect();
    if gone.is_empty() {
        None
    } else {
        Some(format!("Missing: {}", gone.join(" ")))
    }
}

fn have(program: &str) -> bool {
    crate::files::open::on_path(&std::env::var("PATH").unwrap_or_default(), program)
}

fn sudo_available() -> bool {
    sudo(&["true"]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudo_available_works() {
        // Runs without panicking regardless of container sandbox privileges.
        let _ = sudo_available();
    }
}
