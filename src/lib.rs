#![forbid(unsafe_code)]

#[macro_use] mod macros;
#[cfg(    feature = "manifest") ] mod manifest;
#[cfg(not(feature = "manifest"))] mod manifest { pub(super) fn find_cwd_installs() -> Result<Vec<crate::InstallSet>, crate::Error> { Ok(Vec::new()) } }

use std::env::ArgsOs;
use std::fmt::{self, Display, Debug, Formatter, Write as _};
use std::ffi::{OsStr, OsString};
use std::hash::*;
use std::io::{self, BufRead, BufReader};
use std::path::*;
use std::process::{Command, Stdio};



/// An opaque `cargo-local-install` error, currently meant for [Display] only.
pub struct Error(String, Option<Inner>);
impl Display for Error { fn fmt(&self, fmt: &mut Formatter) -> fmt::Result { write!(fmt, "{}", self.0) } }
impl Debug   for Error { fn fmt(&self, fmt: &mut Formatter) -> fmt::Result { write!(fmt, "Error({:?})", self.0) } }
impl std::error::Error for Error {}

enum Inner { Io(io::Error) }
impl From<io::Error> for Inner { fn from(err: io::Error) -> Self { Inner::Io(err) } }



#[derive(Clone, Copy, PartialEq, Eq)]
enum LogMode {
    Quiet,
    Normal,
    Verbose,
}

#[derive(Debug)]
struct InstallSet {
    bin:        PathBuf,
    src:        Option<PathBuf>,
    installs:   Vec<Install>,
}

impl InstallSet {
    fn any_local(&self) -> bool { self.installs.iter().any(|i| i.is_local()) }
    fn any_remote(&self) -> bool { self.installs.iter().any(|i| i.is_remote()) }
}

#[derive(Debug)]
struct Install {
    name:   OsString,
    flags:  Vec<InstallFlag>,
}

impl Install {
    fn is_local(&self) -> bool { self.flags.iter().any(|flag| flag.flag == "--path") }
    fn is_remote(&self) -> bool { !self.is_local() }
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
struct InstallFlag {
    flag: OsString,
    args: Vec<OsString>,
}
impl InstallFlag {
    fn new(flag: impl Into<OsString>, args: Vec<OsString>) -> Self { Self { flag: flag.into(), args } }
}

/// Run an install after reading the executable name / subcommand.
/// Will `exit(...)`.
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
    run_from_args_os_after_exe(args).unwrap_or_else(|err| fatal!("{}", err));
    std::process::exit(0);
}

/// Run an install after reading the executable name / subcommand.
///
/// ## Example
/// ```no_run
/// fn main() {
///     let mut args = std::env::args_os();
///     let _cargo_exe  = args.next(); // "cargo.exe"
///     let _subcommand = args.next(); // "local-install"
///     cargo_local_install::run_from_args_os_after_exe(args).unwrap();
/// }
/// ```
pub fn run_from_args_os_after_exe(args: ArgsOs) -> Result<(), Error> {
    run_from_strs(args)
}

/// Run an install based on string arguments.
///
/// ## Example
/// ```no_run
/// # fn a() -> Result<(), Box<dyn std::error::Error>> {
/// # use std::ffi::*;
/// # use cargo_local_install::run_from_strs;
/// // &str s
/// run_from_strs(["cargo-web", "--version", "^0.6"].iter())?;
/// run_from_strs(["cargo-web", "--version", "^0.6"].into_iter())?;
///
/// // String s
/// let s = ["cargo-web", "--version", "^0.6"];
/// let s = s.iter().copied().map(String::from).collect::<Vec<String>>();
/// run_from_strs(s.iter())?;
/// run_from_strs(s.into_iter())?;
/// 
/// // &OsStr s
/// let os = ["cargo-web", "--version", "^0.6"];
/// let os = os.iter().map(OsStr::new).collect::<Vec<&OsStr>>();
/// run_from_strs(os.iter())?;
/// run_from_strs(os.into_iter())?;
/// 
/// // OsString s
/// let os = ["cargo-web", "--version", "^0.6"];
/// let os = os.iter().map(OsString::from).collect::<Vec<OsString>>();
/// run_from_strs(os.iter())?;
/// run_from_strs(os.into_iter())?;
/// # Ok(())
/// # }
/// ```
pub fn run_from_strs<Args: Iterator<Item = Arg>, Arg: Into<OsString> + AsRef<OsStr>>(args: Args) -> Result<(), Error> {
    let start = std::time::Instant::now();

    // XXX: I'll likely relax either "Into<OsString>" or "AsRef<OsStr>", but I haven't decided which just yet.
    let mut args = args.peekable();

    let mut dry_run     = false;
    let mut path_warning= true;
    let mut log_mode    = LogMode::Normal;
    let mut locked      = None;
    let mut dst_bin     = PathBuf::from("bin");
    let mut target_dir  = None;
    let mut path        = None;

    let mut options     = Vec::<InstallFlag>::new(); // will get reordered for improved caching
    let mut crates      = Vec::<OsString>::new();

    while let Some(arg) = args.next() {
        let arg = arg.into();
        let lossy = arg.to_string_lossy();
        match &*lossy {
            "--help"        => return help(),
            //"--version"     => return version(), // XXX: Conflicts with version selection flag

            // We want to warn if `--locked` wasn't passed, since you probably wanted it
            "--locked"      => locked = Some(true ),
            "--unlocked"    => locked = Some(false), // new to cargo-local-install

            // Custom-handled flags
            "--root"        => dst_bin      = PathBuf::from(args.next().ok_or_else(|| error!(None, "--root must specify a directory"))?.into()).join("bin"),
            "--out-bin"     => dst_bin      = PathBuf::from(args.next().ok_or_else(|| error!(None, "--out-bin must specify a directory"))?.into()), // new to cargo-local-install
            "--target-dir"  => target_dir   = Some(canonicalize(PathBuf::from(args.next().ok_or_else(|| error!(None, "--target-dir must specify a directory"))?.into()))?),
            "--path"        => path         = Some(canonicalize(PathBuf::from(args.next().ok_or_else(|| error!(None, "--path must specify a directory"))?.into()))?),
            "--list"        => return Err(error!(None, "not yet implemented: --list (should this list global cache or local bins?)")),
            "--no-track"    => return Err(error!(None, "not yet implemented: --no-track (the entire point of this crate is tracking...)")),
            "-Z"            => return Err(error!(None, "not yet implemented: -Z flags")),
            "--frozen"      => return Err(error!(None, "not yet implemented: --frozen (last I checked this never worked in cargo install anyways?)")), // https://github.com/rust-lang/cargo/issues/7169#issuecomment-515195574
            "--offline"     => return Err(error!(None, "not yet implemented: --offline")),
            "--dry-run"     => dry_run = true, // new to cargo-local-install
            "--no-path-warning" => path_warning = false, // new to cargo-local-install

            // pass-through single-arg commands
            "-q" | "--quiet" => {
                log_mode = LogMode::Quiet;
                options.push(InstallFlag::new(arg, Vec::new()));
            },
            "-v" | "--verbose" => {
                log_mode = LogMode::Verbose;
                options.push(InstallFlag::new(arg, Vec::new()));
            },
            "-j" | "--jobs" |
            "-f" | "--force" |
            "--all-features" | "--no-default-features" |
            "--debug" | "--bins" | "--examples"
            => {
                options.push(InstallFlag::new(arg, Vec::new()));
            },

            // pass-through single-arg commands
            "--version" |
            "--git" | "--branch" | "--tag" | "--rev" |
            "--profile" | "--target" |
            "--index" | "--registry" |
            "--color"
            => {
                let arg2 = args.next().ok_or_else(|| error!(None, "{} requires an argument", lossy))?.into();
                options.push(InstallFlag::new(arg, vec![arg2]));
            },

            // pass-through multi-arg commands
            "--features"    => return Err(error!(None, "not yet implemented: {}", lossy)),
            "--bin"         => return Err(error!(None, "not yet implemented: {}", lossy)),
            "--example"     => return Err(error!(None, "not yet implemented: {}", lossy)),

            "--" => {
                crates.extend(args.map(|a| a.into()));
                break;
            },

            flag if flag.starts_with("-") => return Err(error!(None, "unrecognized flag: {}", flag)),
            _krate => crates.push(arg),
        }
    }
    let quiet   = log_mode == LogMode::Quiet;
    let verbose = log_mode == LogMode::Verbose;

    let locked = locked.unwrap_or_else(|| {
        if !crates.is_empty() { warnln!("either specify --locked to use the same dependencies the crate was built with, or --unlocked to get rid of this warning"); }
        false
    });
    if locked {
        options.push(InstallFlag::new("--locked", Vec::new()));
    }

    let mut installs = if crates.is_empty() {
        manifest::find_cwd_installs().map_err(|err| error!(None, "error enumerating Cargo.tomls: {}", err))?
    } else {
        vec![InstallSet {
            bin:        dst_bin.clone(),
            src:        None,
            installs:   crates.into_iter().map(|c| Install { name: c, flags: vec![] }).collect(),
        }]
    };

    if installs.is_empty() {
        return Err(error!(None, "no crates specified"))
    }

    let global_dir = {
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let mut d = PathBuf::from(std::env::var_os(var).ok_or_else(|| error!(None, "couldn't determine target dir, {} not set", var))?);
        d.push(".cargo");
        d.push("local-install");
        d
    };
    let crates_cache_dir = global_dir.join("crates");

    let target_dir = target_dir.map_or_else(|| Ok(global_dir.join("target")), |td| canonicalize(td))?;
    options.push(InstallFlag::new("--target-dir", vec![target_dir.into()]));
    if let Some(path) = path { options.push(InstallFlag::new("--path", vec![canonicalize(path)?.into()])); }
    options.sort();

    for set in installs.iter_mut() {
        for install in set.installs.iter_mut() {
            install.flags.extend(options.clone());
            install.flags.sort();
        }
    }

    if !dry_run { std::fs::create_dir_all(&dst_bin).map_err(|err| error!(err, "unable to create {}: {}", dst_bin.display(), err))? }

    for set in installs.into_iter() {
        let any_local  = set.any_local();
        let any_remote = set.any_remote();
        if set.installs.is_empty() { continue }
        assert!(any_local || any_remote);

        let built = set.bin.join(".built");

        let up_to_date = if !any_remote {
            false
        } else if let Some(src) = set.src.as_ref() {
            let src_mod = src.metadata().ok().and_then(|m| m.modified().ok());
            let built_mod = built.metadata().ok().and_then(|m| m.modified().ok());

            let up_to_date = match (src_mod, built_mod) {
                (Some(src), Some(built))    => src < built,
                _other                      => false,
            };

            if up_to_date && !any_local {
                if verbose { statusln!("Skipping", "`{}`: up to date", src.display()); }
                continue
            }

            up_to_date
        } else {
            false
        };

        for install in set.installs.into_iter() {
            if install.is_remote() {
                if up_to_date { continue }
            }
            let context = Context { dry_run, quiet, verbose, crates_cache_dir: crates_cache_dir.as_path(), dst_bin: set.bin.as_path() };
            install.install(context)?;
        }
        if any_remote && set.src.is_some() {
            std::fs::write(&built, "").map_err(|err| error!(err, "unable to create {}: {}", built.display(), err))?;
        }
    }

    let stop = std::time::Instant::now();
    if !quiet { statusln!("Finished", "installing crate(s) in {:.2}s", (stop-start).as_secs_f32()); }
    if path_warning { warnln!("be sure to add `{}` to your PATH to be able to run the installed binaries", dst_bin.display()); }
    Ok(())
}

struct Context<'a> {
    pub dry_run:            bool,
    pub quiet:              bool,
    pub verbose:            bool,
    pub crates_cache_dir:   &'a Path,
    pub dst_bin:            &'a Path,
}

impl Install {
    fn install(self, context: Context) -> Result<(), Error> {
        let Context { dry_run, quiet, verbose, crates_cache_dir, dst_bin } = context;

        let mut trace = format!("cargo install");
        let mut cmd = Command::new("cargo");
        cmd.arg("install");
        for InstallFlag { flag, args } in self.flags {
            write!(&mut trace, " {}", flag.to_str().unwrap()).unwrap();
            cmd.arg(flag);
            for arg in args.into_iter() {
                write!(&mut trace, " {:?}", arg).unwrap();
                cmd.arg(arg);
            }
        }

        let hash = {
            // real trace will have "--root ...", but that depends on hash!
            let trace_for_hash = format!("{} -- {}", trace, self.name.to_string_lossy());
            #[allow(deprecated)] let mut hasher = std::hash::SipHasher::new();
            trace_for_hash.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        };

        let krate_build_dir = crates_cache_dir.join(hash);
        write!(&mut trace, " --root {:?}", krate_build_dir.display()).unwrap();
        cmd.arg("--root").arg(&krate_build_dir);

        write!(&mut trace, " --color always").unwrap();
        cmd.arg("--color").arg("always");

        trace.push_str(" -- ");
        trace.push_str(&self.name.to_string_lossy());
        cmd.arg("--");
        cmd.arg(self.name);

        if verbose { statusln!("Running", "`{}`", trace) }
        if !dry_run {
            cmd.stderr(Stdio::piped());
            let mut cmd = cmd.spawn().map_err(|err| error!(err, "failed to spawn {}: {}", trace, err))?;
            let stderr_thread = cmd.stderr.take().map(|stderr| std::thread::spawn(|| filter_stderr(stderr)));
            let status = cmd.wait();
            let _stderr_thread = stderr_thread.map(|t| t.join());
            let status = status.map_err(|err| error!(err, "failed to execute {}: {}", trace, err))?;
            match status.code() {
                Some(0) => { if verbose { statusln!("Succeeded", "`{}`", trace) } },
                Some(n) => return Err(error!(None, "{} failed (exit code {})", trace, n)),
                None    => return Err(error!(None, "{} failed (signal)", trace)),
            }
        } else { // dry_run
            statusln!("Skipped", "`{}` (--dry-run)", trace);
            return Ok(()); // XXX: Would be nice to log copied bins, but without building them we don't know what they are
        }

        let src_bin_path = krate_build_dir.join("bin");
        let src_bins = src_bin_path.read_dir().map_err(|err| error!(err, "unable to enumerate source bins at {}: {}", src_bin_path.display(), err))?;
        for src_bin in src_bins {
            let src_bin = src_bin.map_err(|err| error!(err, "error enumerating source bins at {}: {}", src_bin_path.display(), err))?;
            let dst_bin = dst_bin.join(src_bin.file_name());
            let file_type = src_bin.file_type().map_err(|err| error!(err, "error determining file type for {}: {}", src_bin.path().display(), err))?;
            if !file_type.is_file() { continue }
            let src_bin = src_bin.path();

            if verbose { statusln!("Replacing", "`{}`", dst_bin.display()) }
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
            std::fs::copy(&src_bin, &dst_bin).map_err(|err| error!(err, "error replacing `{}` with `{}`: {}", dst_bin.display(), src_bin.display(), err))?;
            if !quiet { statusln!("Replaced", "`{}` with `{}`", dst_bin.display(), src_bin.display()) }
        }

        Ok(())
    }
}

struct Ignore {
    /// ASCII prefix
    pre:    &'static str,

    /// ANSI colored prefix
    prec:   &'static str,

    /// postfix
    post:   &'static str,
}

static IGNORE : &'static [Ignore] = &[
    // We spam reinstalls for already installed stuff
    Ignore { pre: "     Ignored package `", post: "` is already installed, use --force to override", prec: "\u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[32m     Ignored\u{1b}[0m package `" },

    // We spam "internal" .cargo\local-install paths
    Ignore { pre: "warning: be sure to add `", post: "` to your PATH to be able to run the installed binaries", prec: "\x1B[0m\x1B[0m\x1B[1m\x1B[33mwarning\x1B[0m\x1B[1m:\x1B[0m be sure to add `" },
    Ignore { pre: "   Replacing ", post: "", prec: "\u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[32m   Replacing\u{1b}[0m " },
    Ignore { pre: "    Replaced ", post: "", prec: "\u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[32m    Replaced\u{1b}[0m " },

    // Don't spam this per-crate that's silly, roll our own for the final output
    Ignore { pre: "    Finished ", post: "", prec: "\u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[32m    Finished\u{1b}[0m " },

    // Okay, we'll let *this* spam through...
    //Ignore { pre: "  Installing ", post: "", prec: "\u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[32m  Installing\u{1b}[0m " },
];



/// Filters out bad warnings like:
/// "\u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[33mwarning\u{1b}[0m\u{1b}[1m:\u{1b}[0m be sure to add `C:\\Users\\Name\\.cargo\\local-install\\crates\\e5ce6d367e4d6f3f\\bin` to your PATH to be able to run the installed binaries"
fn filter_stderr(input: std::process::ChildStderr) -> io::Result<()> {
    for line in BufReader::new(input).lines() {
        let line = line?;
        if IGNORE.iter().any(|ignore| line.ends_with(ignore.post) && (line.starts_with(ignore.pre) || line.starts_with(ignore.prec))) { continue }
        eprintln!("{}", line);
    }
    Ok(())
}

fn help() -> Result<(), Error> {
    print_usage(&mut std::io::stdout().lock()).map_err(|err| error!(err, "unable to write help text to stdout: {}", err))
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
    writeln!(o, "        --root <DIR>                                 Install package bins into <DIR>/bin")?;
    writeln!(o, "        --out-bin <DIR>                              Install package bins into <DIR>")?;
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
    writeln!(o, "        --no-path-warning                            Don't remind the user to add `bin` to their PATH")?;
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

fn canonicalize(path: impl AsRef<Path>) -> Result<PathBuf, Error> {
    let path = path.as_ref();
    let path = std::fs::canonicalize(path).map_err(|err| error!(err, "unable to canonicalize {}: {}", path.display(), err))?;
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
    Ok(o)
}
