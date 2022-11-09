#![forbid(unsafe_code)]

fn main() {
    let mut args = std::env::args_os();
    let _exe = args.next(); // cargo-local-install.exe
    let local_install = args.next().unwrap_or_default(); // typically "local-install"
    if local_install != "local-install" {
        // Directly launched instead of launched through cargo?
        args = std::env::args_os(); // reset
        let _exe = args.next(); // only skip exe, not "incorrect" subcommand
    }
    cargo_local_install::exec_from_args_os_after_exe(args);
}
