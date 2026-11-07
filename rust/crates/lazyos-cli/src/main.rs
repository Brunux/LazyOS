//! The `lazyos` binary: everything is in the library so the tests can drive the
//! dispatch, the host decision and the paths without a process.

fn main() {
    // Ensure ~/.config/bin, ~/.local/bin, /usr/sbin and /sbin are on PATH so that
    // session helpers (e.g. lazyos-profile) and system tools (blkid, mkswap) are
    // always discoverable even when greetd or a wrapper starts with a stripped PATH.
    if let Ok(path) = std::env::var("PATH") {
        let mut new_path = path.clone();
        if let Ok(home) = std::env::var("HOME") {
            let config_bin = format!("{home}/.config/bin");
            if !new_path.split(':').any(|p| p == config_bin) {
                new_path = format!("{config_bin}:{new_path}");
            }
            let local_bin = format!("{home}/.local/bin");
            if !new_path.split(':').any(|p| p == local_bin) {
                new_path.push_str(&format!(":{local_bin}"));
            }
        }
        if !new_path.split(':').any(|p| p == "/usr/sbin") {
            new_path.push_str(":/usr/sbin");
        }
        if !new_path.split(':').any(|p| p == "/sbin") {
            new_path.push_str(":/sbin");
        }
        if new_path != path {
            // Safety: Called at the very start of main before any threads are spawned.
            unsafe {
                std::env::set_var("PATH", new_path);
            }
        }
    }

    let argv: Vec<String> = std::env::args().collect();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "lazyos".to_string());
    let env = lazyos_cli::host::Env::from_process(&argv0);
    std::process::exit(lazyos_cli::run(argv, &env));
}
