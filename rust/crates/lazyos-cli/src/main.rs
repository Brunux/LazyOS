//! The `lazyos` binary: everything is in the library so the tests can drive the
//! dispatch, the host decision and the paths without a process.

fn main() {
    // Append /usr/sbin and /sbin if not already present, just as lib-rofi.sh does.
    if let Ok(path) = std::env::var("PATH") {
        let mut new_path = path.clone();
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
