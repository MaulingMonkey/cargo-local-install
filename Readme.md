# cargo-local-install

Wraps `cargo install` for better local, non-conflicting installation

[![GitHub](https://img.shields.io/github/stars/MaulingMonkey/cargo-local-install.svg?label=GitHub&style=social)](https://github.com/MaulingMonkey/cargo-local-install)
[![crates.io](https://img.shields.io/crates/v/cargo-local-install.svg)](https://crates.io/crates/cargo-local-install)
[![docs.rs](https://docs.rs/cargo-local-install/badge.svg)](https://docs.rs/cargo-local-install)
[![%23![forbid(unsafe_code)]](https://img.shields.io/github/search/MaulingMonkey/cargo-local-install/unsafe%2bextension%3Ars?color=green&label=%23![forbid(unsafe_code)])](https://github.com/MaulingMonkey/cargo-local-install/search?q=forbid%28unsafe_code%29+extension%3Ars)
[![rust: stable](https://img.shields.io/badge/rust-stable-yellow.svg)](https://gist.github.com/MaulingMonkey/c81a9f18811079f19326dac4daa5a359#minimum-supported-rust-versions-msrv)
[![License](https://img.shields.io/crates/l/cargo-local-install.svg)](https://github.com/MaulingMonkey/cargo-local-install)
[![Build Status](https://travis-ci.com/MaulingMonkey/cargo-local-install.svg?branch=master)](https://travis-ci.com/MaulingMonkey/cargo-local-install)

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
*   Creates a global cache of binaries, but installs a link/copy in `./bin` by default.



<h2 name="license">License</h2>

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.



<h2 name="contribution">Contribution</h2>

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
