#![forbid(unsafe_code)]

#[macro_use] mod macros;

use std::env::ArgsOs;
use std::fmt::Write;
use std::ffi::OsString;
use std::hash::*;
use std::io;
use std::path::*;
use std::process::{Command, exit};



#[derive(Clone, Copy, PartialEq, Eq)]
enum LogMode {
    Quiet,
    Normal,
    Verbose,
}

/// Run an install after reading the executable name / subcommand.
/// Will exit(...).
///
/// ## Example
/// ```no_run
/// fn main() {
///     let mut args = std::env::args_os();
///     let _cargo_exe  = args.next(); // "cargo.exe"
///     let _subcommand = args.next(); // "local-install"
///     cargo_local_install::exec_from_args_os_after_exe(args);
/// }
/// ```
pub fn exec_from_args_os_after_exe(args: ArgsOs) -> ! {
    let mut args = args.peekable();

    let mut dry_run     = false;
    let mut log_mode    = LogMode::Normal;
    let mut locked      = None;
    let mut root        = None;
    let mut target_dir  = None;
    let mut path        = None;

    let mut options     = Vec::<(OsString, Vec<OsString>)>::new(); // will get reordered for improved caching
    let mut crates      = Vec::<OsString>::new();

    let mut any = false;
    while let Some(arg) = args.next() {
        any = true;
        let lossy = arg.to_string_lossy();
        match &*lossy {
            "--help"        => help(),
            //"--version"     => version(), // XXX: Conflicts with version selection flag

            // We want to warn if `--locked` wasn't passed, since you probably wanted it
            "--locked"      => locked = Some(true ),
            "--unlocked"    => locked = Some(false), // new to cargo-local-install

            // Custom-handled flags
            "--root"        => root         = Some(PathBuf::from(args.next().unwrap_or_else(|| fatal!("--root must specify a directory")))),
            "--target-dir"  => target_dir   = Some(canonicalize(PathBuf::from(args.next().unwrap_or_else(|| fatal!("--target-dir must specify a directory"))))),
            "--path"        => path         = Some(canonicalize(PathBuf::from(args.next().unwrap_or_else(|| fatal!("--path must specify a directory"))))),
            "--list"        => fatal!("not yet implemented: --list (should this list global cache or local bins?)"),
            "--no-track"    => fatal!("not yet implemented: --no-track (the entire point of this crate is tracking...)"),
            "-Z"            => fatal!("not yet implemented: -Z flags"),
            "--frozen"      => fatal!("not yet implemented: --frozen (last I checked this never worked in cargo install anyways?)"), // https://github.com/rust-lang/cargo/issues/7169#issuecomment-515195574
            "--offline"     => fatal!("not yet implemented: --offline"),
            "--dry-run"     => dry_run = true, // new to cargo-local-install

            // pass-through single-arg commands
            "-q" | "--quiet" => {
                log_mode = LogMode::Quiet;
                options.push((arg, Vec::new()));
            },
            "-v" | "--verbose" => {
                log_mode = LogMode::Verbose;
                options.push((arg, Vec::new()));
            },
            "-j" | "--jobs" |
            "-f" | "--force" |
            "--all-features" | "--no-default-features" |
            "--debug" | "--bins" | "--examples"
            => {
                options.push((arg, Vec::new()));
            },

            // pass-through single-arg commands
            "--version" |
            "--git" | "--branch" | "--tag" | "--rev" |
            "--profile" | "--target" |
            "--index" | "--registry" |
            "--color"
            => {
                let arg2 = args.next().unwrap_or_else(|| fatal!("{} requires an argument", lossy));
                options.push((arg, vec![arg2]));
            },

            // pass-through multi-arg commands
            "--features"    => fatal!("not yet implemented: {}", lossy),
            "--bin"         => fatal!("not yet implemented: {}", lossy),
            "--example"     => fatal!("not yet implemented: {}", lossy),

            "--" => {
                crates.extend(args);
                break;
            },

            flag if flag.starts_with("-") => fatal!("unrecognized flag: {}", flag),
            _krate => crates.push(arg),
        }
    }
    if !any {
        let _ = print_usage(std::io::stderr().lock());
        exit(1);
    }
    let quiet   = log_mode == LogMode::Quiet;
    let verbose = log_mode == LogMode::Verbose;

    let locked = locked.unwrap_or_else(|| {
        warnln!("either specify --locked to use the same dependencies the crate was built with, or --unlocked to get rid of this warning");
        false
    });
    if locked {
        options.push(("--locked".into(), Vec::new()));
    }

    if crates.is_empty() { fatal!("no crates specified") }

    let global_dir = {
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let mut d = PathBuf::from(std::env::var_os(var).unwrap_or_else(|| fatal!("couldn't determine target dir, {} not set", var)));
        d.push(".cargo");
        d.push("local-install");
        d
    };
    let crates_cache_dir = global_dir.join("crates");

    options.push(("--target-dir".into(), vec![target_dir.map(|td| canonicalize(td)).unwrap_or_else(|| global_dir.join("target")).into()]));
    if let Some(path) = path { options.push(("--path".into(), vec![canonicalize(path).into()])); }
    options.sort();
    let options = options;

    let root = root.unwrap_or_else(|| PathBuf::new());
    let dst_bin = root.join("bin");
    std::fs::create_dir_all(&dst_bin).unwrap_or_else(|err| fatal!("unable to create {}: {}", dst_bin.display(), err));

    for krate in crates.into_iter() {
        let mut trace = format!("cargo install");
        let mut cmd = Command::new("cargo");
        cmd.arg("install");
        for (flag, args) in options.iter() {
            write!(&mut trace, " {}", flag.to_str().unwrap()).unwrap();
            cmd.arg(flag);
            for arg in args.into_iter() {
                write!(&mut trace, " {:?}", arg).unwrap();
                cmd.arg(arg);
            }
        }

        let hash = {
            // real trace will have "--root ...", but that depends on hash!
            let trace_for_hash = format!("{} -- {}", trace, krate.to_string_lossy());
            #[allow(deprecated)] let mut hasher = std::hash::SipHasher::new();
            trace_for_hash.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        };

        let krate_build_dir = crates_cache_dir.join(hash);
        write!(&mut trace, " --root {:?}", krate_build_dir.display()).unwrap();
        cmd.arg("--root").arg(&krate_build_dir);

        trace.push_str(" -- ");
        trace.push_str(&krate.to_string_lossy());
        cmd.arg("--");
        cmd.arg(krate);

        if verbose { statusln!("Running", "`{}`", trace) }
        if !dry_run {
            let status = cmd.status().unwrap_or_else(|err| fatal!("failed to execute {}: {}", trace, err));
            match status.code() {
                Some(0) => { if verbose { statusln!("Succeeded", "`{}`", trace) } },
                Some(n) => fatal!("{} failed (exit code {})", trace, n),
                None    => fatal!("{} failed (signal)", trace),
            }
        } else { // dry_run
            statusln!("Skipped", "`{}` (--dry-run)", trace);
            continue; // XXX: Would be nice to log copied bins, but without building them we don't know what they are
        }

        let src_bin_path = krate_build_dir.join("bin");
        let src_bins = src_bin_path.read_dir().unwrap_or_else(|err| fatal!("unable to enumerate source bins at {}: {}", src_bin_path.display(), err));
        for src_bin in src_bins {
            let src_bin = src_bin.unwrap_or_else(|err| fatal!("error enumerating source bins at {}: {}", src_bin_path.display(), err));
            let dst_bin = dst_bin.join(src_bin.file_name());
            let file_type = src_bin.file_type().unwrap_or_else(|err| fatal!("error determining file type for {}: {}", src_bin.path().display(), err));
            if !file_type.is_file() { continue }
            let src_bin = src_bin.path();

            if !quiet { statusln!("Replacing", "`{}`", dst_bin.display()) }
            #[cfg(windows)] {
                let _ = std::fs::remove_file(&dst_bin);
                if let Err(err) = std::os::windows::fs::symlink_file(&src_bin, &dst_bin) {
                    if !quiet { warnln!("Unable link `{}` to `{}`: {}", dst_bin.display(), src_bin.display(), err) }
                } else {
                    if !quiet { statusln!("Linked", "`{}` to `{}`", dst_bin.display(), src_bin.display()) }
                    continue
                }
            }
            #[cfg(unix)] {
                let _ = std::fs::remove_file(&dst_bin);
                if let Err(err) = std::os::unix::fs::symlink(&src_bin, &dst_bin) {
                    if !quiet { warnln!("Unable link `{}` to `{}`: {}", dst_bin.display(), src_bin.display(), err) }
                } else {
                    if !quiet { statusln!("Linked", "`{}` to `{}`", dst_bin.display(), src_bin.display()) }
                    continue
                }
            }
            std::fs::copy(&src_bin, &dst_bin).unwrap_or_else(|err| fatal!("error replacing `{}` with `{}`: {}", dst_bin.display(), src_bin.display(), err));
            if !quiet { statusln!("Replaced", "`{}` with `{}`", dst_bin.display(), src_bin.display()) }
        }
    }

    warnln!("be sure to add `{}` to your PATH to be able to run the installed binaries", dst_bin.display());
    exit(0);
}

fn help() {
    let _ = print_usage(&mut std::io::stdout().lock());
    exit(0);
}

fn print_usage(mut o: impl io::Write) -> io::Result<()> {
    let o = &mut o;
    writeln!(o, "cargo-local-install")?;
    writeln!(o, "Install a Rust binary. Default installation location is ./bin")?;
    writeln!(o)?;
    writeln!(o, "USAGE:")?;
    writeln!(o, "    cargo local-install [OPTIONS] [--] [crate]...")?;
    writeln!(o, "    cargo-local-install [OPTIONS] [--] [crate]...")?;
    writeln!(o)?;
    writeln!(o, "OPTIONS:")?;
    // pass-through options to `cargo install`
    writeln!(o, "    -q, --quiet                                      No output printed to stdout")?;
    writeln!(o, "        --version <VERSION>                          Specify a version to install")?;
    writeln!(o, "        --git <URL>                                  Git URL to install the specified crate from")?;
    writeln!(o, "        --tag <TAG>                                  Tag to use when installing from git")?;
    writeln!(o, "        --rev <SHA>                                  Specific commit to use when installing from git")?;
    writeln!(o, "        --path <PATH>                                Filesystem path to local crate to install")?;
    // writeln!(o, "        --list                                       list all installed packages and their versions // not supported
    writeln!(o, "    -j, --jobs <N>                                   Number of parallel jobs, defaults to # of CPUs")?;
    writeln!(o, "    -f, --force                                      Force overwriting existing crates or binaries")?;
    // writeln!(o, "        --no-track                                   Do not save tracking information")?; // not supported
    // writeln!(o, "        --features <FEATURES>...                     Space or comma separated list of features to activate")?; // nyi
    writeln!(o, "        --all-features                               Activate all available features")?;
    writeln!(o, "        --no-default-features                        Do not activate the `default` feature")?;
    writeln!(o, "        --profile <PROFILE-NAME>                     Install artifacts with the specified profile")?;
    writeln!(o, "        --debug                                      Build in debug mode instead of release mode")?;
    // writeln!(o, "        --bin <NAME>...                              Install only the specified binary")?; // nyi
    writeln!(o, "        --bins                                       Install all binaries")?;
    // writeln!(o, "        --example <NAME>...                          Install only the specified example")?; // nyi
    writeln!(o, "        --examples                                   Install all examples")?;
    writeln!(o, "        --target <TRIPLE>                            Build for the target triple")?;
    writeln!(o, "        --target-dir <DIRECTORY>                     Directory for all generated artifacts")?;
    writeln!(o, "        --root <DIR>                                 Directory to install packages into")?;
    writeln!(o, "        --index <INDEX>                              Registry index to install from")?;
    writeln!(o, "        --registry <REGISTRY>                        Registry to use")?;
    writeln!(o, "    -v, --verbose                                    Use verbose output (-vv very verbose/build.rs output)")?;
    writeln!(o, "        --color <WHEN>                               Coloring: auto, always, never")?;
    // writeln!(o, "        --frozen                                     Require Cargo.lock and cache are up to date")?; // not supported
    writeln!(o, "        --locked                                     Require Cargo.lock is up to date")?;
    // writeln!(o, "        --offline                                    Run without accessing the network")?; // not supported
    // CUSTOM FLAGS:
    writeln!(o, "        --unlocked                                   Don't require an up-to-date Cargo.lock")?;
    writeln!(o, "        --dry-run                                    Print `cargo install ...` spam but don't actually install")?;
    // writeln!(o, "    -Z <FLAG>...")?; // nyi
    writeln!(o)?;
    writeln!(o, "ARGS:")?;
    writeln!(o, "    <crate>...")?;
    writeln!(o)?;
    writeln!(o, "This command wraps `cargo install` to solve a couple of problems with using")?;
    writeln!(o, "the basic command directly:")?;
    writeln!(o)?;
    writeln!(o, "* The global `~/.cargo/bin` directory can contain only a single installed")?;
    writeln!(o, "  version of a package at a time - if you've got one project relying on")?;
    writeln!(o, "  `cargo web 0.5` and another prjoect relying on `cargo web 0.6`, you're SOL.")?;
    writeln!(o)?;
    writeln!(o, "* Forcing local installs with `--root my/project` to avoid global version")?;
    writeln!(o, "  conflicts means you must rebuild the entire dependency for each project,")?;
    writeln!(o, "  even when you use the exact same version for 100 other projects before.")?;
    writeln!(o)?;
    writeln!(o, "* When building similar binaries, the lack of target directory caching means")?;
    writeln!(o, "  the entire dependency tree must still be rebuilt from scratch.")?;
    Ok(())
}

#[allow(dead_code)]
fn version() {
    // TODO: (git hash, mod status, date) via build.rs nonsense?
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

fn canonicalize(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let path = std::fs::canonicalize(path).unwrap_or_else(|err| fatal!("unable to canonicalize {}: {}", path.display(), err));
    let mut o = PathBuf::new();
    for component in path.components() {
        if let Component::Prefix(pre) = component {
            match pre.kind() {
                Prefix::VerbatimDisk(disk)  => o.push(format!("{}:", disk as char)),
                _other                      => o.push(component),
            }
        } else {
            o.push(component);
        }
    }
    o
}
