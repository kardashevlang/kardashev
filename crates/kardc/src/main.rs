//! `kard` — the kardashev toolchain entry point.
//!
//! A single binary that is the compiler, build system, test runner and
//! formatter. All real logic lives in [`kardc::cli`].

use std::process::ExitCode;

fn main() -> ExitCode {
    kardc::cli::run(std::env::args().skip(1).collect())
}
