# cargo-local-install

Wraps `cargo install` for better local, non-conflicting installation

[![GitHub](https://img.shields.io/github/stars/MaulingMonkey/cargo-local-install.svg?label=GitHub&style=social)](https://github.com/MaulingMonkey/cargo-local-install)
[![crates.io](https://img.shields.io/crates/v/cargo-local-install.svg)](https://crates.io/crates/cargo-local-install)
[![docs.rs](https://docs.rs/cargo-local-install/badge.svg)](https://docs.rs/cargo-local-install)
[![%23![forbid(unsafe_code)]](https://img.shields.io/github/search/MaulingMonkey/cargo-local-install/unsafe%2bextension%3Ars?color=green&label=%23![forbid(unsafe_code)])](https://github.com/MaulingMonkey/cargo-local-install/search?q=forbid%28unsafe_code%29+extension%3Ars)
[![rust: stable](https://img.shields.io/badge/rust-stable-yellow.svg)](https://gist.github.com/MaulingMonkey/c81a9f18811079f19326dac4daa5a359#minimum-supported-rust-versions-msrv)
[![License](https://img.shields.io/crates/l/cargo_local_install.svg)](https://github.com/MaulingMonkey/cargo-local-install)
[![Build Status](https://travis-ci.com/MaulingMonkey/cargo-local-install.svg?branch=master)](https://travis-ci.com/MaulingMonkey/cargo-local-install)

Want to script `cargo install cargo-web --version 0.6 --root my-project` to avoid version conflicts with other projects?<br>
Hate having a million copies of `cargo-web 0.6.26` and waiting for it to build if you go down that route?<br>
`cargo-local-install` now has your back, and will create symlinks into a global cache of reusable bins:

<h2 name="quickstart">Quickstart</h2>

```sh
cargo install cargo-local-install # <400 LOC of rust with 0 dependencies, builds in < 3 seconds on my machine
# slow first builds that create new exes
cargo local-install --locked cargo-web --version "^0.6" --root project-a # symlinks project-a/bin/cargo-web.exe
cargo local-install --locked cargo-web --version "^0.5" --root project-b # symlinks project-b/bin/cargo-web.exe
# fast cached builds that reuse existing exes
cargo local-install --locked cargo-web --version "^0.6" --root project-c # symlinks project-c/bin/cargo-web.exe
cargo local-install --locked cargo-web --version "^0.6"                  # symlinks bin/cargo-web.exe
```

Options are broadly similar to `cargo install`, with a few caveats:
* `--locked` is *strongly* encouraged (warns by default unless it or `--unlocked` is used)
* `--list`, `--no-track`, `--features`, `--bin`, and `--example` are not supported
* `--frozen` and `--offline` are not supported (don't think they worked for `cargo install` either though!)
* `-Z <FLAG>` is not supported



<h2 name="what-why">What? Why?</h2>

`cargo install` is great but suffers a few drawbacks:
*   The global `~/.cargo/bin` directory can contain only a single installed
    version of a package at a time - if you've got one project relying on
    `cargo web 0.5` and another prjoect relying on `cargo web 0.6`, you're SOL.
*   Forcing local installs with `--root my/project` to avoid global version
    conflicts means you must rebuild the entire dependency for each project,
    even when you use the exact same version for 100 other projects before.
*   When building similar binaries, the lack of target directory caching means
    the entire dependency tree must still be rebuilt from scratch.

`cargo local-install` attempts to solve these problems:
*   (Ab)uses `--target-dir` to share built dependencies.
*   Creates a global cache of binaries, but installs a symlink (or copy if that fails) in `./bin` by default.



<h2 name="alternative-sccache">Alternative: sccache</h2>

* [github](https://github.com/mozilla/sccache)
* **Pro**: Can be combined with `cargo-local-install`, no need to pick and choose!
* **Pro**: Cache intermediate built crates for everything, not just installed bins
* **Pro**: Network cache options to share with others
* **Con**: Hundreds of dependencies if you're a weirdo who installs sccache from source
* **Con**: Lots of cache misses, at least when using `cargo install ...`

Some concrete numbers from some local testing:

| with sccache configured for local disk    | time   | notes |
| ----------------------------------------- | ------ | ----- |
| `cargo install cargo-web --root a`        | 3m 38s | cleanish cache, no downloads
| `cargo install cargo-web --root a`        | ~ 1 s  | noop by `cargo install`
| `cargo install cargo-web --root b`        | 1m 21s | many cache failures based on lurching progress speed?

| with local-install (no sccache)           | time   | notes |
| ----------------------------------------- | ------ | ----- |
| `cargo local-install cargo-web --root c`  | 3m 03s | clean cache, no downloads of deps
| `cargo local-install cargo-web --root c`  | ~ 1 s  | noop by `cargo install`
| `cargo local-install cargo-web --root d`  | ~ 1 s  | trivial cache hit by `cargo local-install`



<h2 name="license">License</h2>

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.



<h2 name="contribution">Contribution</h2>

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
