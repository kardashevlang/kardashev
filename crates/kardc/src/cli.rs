//! Command-line dispatch for the `kard` toolchain binary.
//!
//! A single binary that is compiler, build system, test runner and formatter
//! (SPEC §6). Argument parsing is hand-rolled — no external crates — and split
//! into a pure [`parse_args`] step (returning a [`Command`]) and an execution
//! step (one `cmd_*` function per subcommand). The split keeps argument parsing
//! unit-testable without ever invoking the C compiler.
//!
//! Subcommands: `build`, `run`, `test`, `fmt`, `init`, `version`, `help`.

use std::path::Path;
use std::process::ExitCode;

use crate::emit_c::EmitMode;

/// Top-level usage text, printed by `help` (to stdout) and after a usage error
/// (to stderr).
const USAGE: &str = "\
kardashev — a self-contained toolchain for the kardashev systems language.

Usage:
    kard <command> [options]

Commands:
    build [FILE] [-o OUT] [-target TRIPLE]
            Compile a program to a native executable. With no FILE, reads
            ./build.ks for the root source and the output name. OUT defaults
            to the source filename without its `.ks` extension.

    run   [FILE] [-- ARGS...]
            Build to a temporary executable, run it, and propagate its exit
            code. Arguments after `--` are passed through to the program.

    test  [FILE]
            Build and run the test harness; reports pass/fail counts and
            exits non-zero if any test fails.

    fmt   FILE [--check | -w]
            Format source. With no flag, prints canonical source to stdout.
            --check exits non-zero if FILE is not already canonical; -w
            rewrites FILE in place.

    init  [NAME]
            Scaffold a new project. With NAME, creates ./NAME; otherwise
            scaffolds into the current directory.

    version
            Print the toolchain version. (also --version, -V)

    help
            Print this help. (also --help, -h)
";

/// A parsed, validated command line. Produced by [`parse_args`] and consumed by
/// [`run`]; carrying no I/O lets the parser be tested in isolation.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Command {
    /// `build [FILE] [-o OUT] [-target TRIPLE]`
    Build {
        file: Option<String>,
        out: Option<String>,
        target: Option<String>,
    },
    /// `run [FILE] [-- ARGS...]`
    Run {
        file: Option<String>,
        args: Vec<String>,
    },
    /// `test [FILE]`
    Test { file: Option<String> },
    /// `fmt FILE [--check | -w]`
    Fmt { file: String, mode: FmtMode },
    /// `init [NAME]`
    Init { name: Option<String> },
    /// `version` / `--version` / `-V`
    Version,
    /// `help` / `--help` / `-h` / no arguments
    Help,
}

/// What `kard fmt` should do with the formatted output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FmtMode {
    /// Print the canonical source to stdout (default).
    Stdout,
    /// Exit non-zero if the file is not already canonical (writes nothing).
    Check,
    /// Rewrite the file in place.
    Write,
}

/// Entry point invoked by `main`. `args` excludes the program name (the binary
/// passes `std::env::args().skip(1)`), so `args[0]` is the subcommand.
pub fn run(args: Vec<String>) -> ExitCode {
    let cmd = match parse_args(&args) {
        Ok(cmd) => cmd,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!();
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match cmd {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("kardashev {}", crate::VERSION);
            ExitCode::SUCCESS
        }
        Command::Build { file, out, target } => cmd_build(file, out, target),
        Command::Run { file, args } => cmd_run(file, args),
        Command::Test { file } => cmd_test(file),
        Command::Fmt { file, mode } => cmd_fmt(file, mode),
        Command::Init { name } => cmd_init(name),
    }
}

// ---------------------------------------------------------------------------
// Argument parsing (pure; unit-tested below).
// ---------------------------------------------------------------------------

/// Parse the full argument vector into a [`Command`], or an error message
/// describing the misuse. No arguments resolves to `help`.
fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter();
    let sub = match iter.next() {
        Some(s) => s.as_str(),
        None => return Ok(Command::Help),
    };
    let rest: Vec<&str> = iter.map(String::as_str).collect();

    match sub {
        "build" => parse_build(&rest),
        "run" => parse_run(&rest),
        "test" => parse_test(&rest),
        "fmt" => parse_fmt(&rest),
        "init" => parse_init(&rest),
        "version" | "--version" | "-V" => Ok(Command::Version),
        "help" | "--help" | "-h" => Ok(Command::Help),
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

fn parse_build(rest: &[&str]) -> Result<Command, String> {
    let mut file: Option<String> = None;
    let mut out: Option<String> = None;
    let mut target: Option<String> = None;

    let mut i = 0;
    while i < rest.len() {
        let a = rest[i];
        match a {
            "-o" => {
                i += 1;
                let v = rest
                    .get(i)
                    .ok_or_else(|| "flag `-o` requires an argument".to_string())?;
                out = Some((*v).to_string());
            }
            "-target" => {
                i += 1;
                let v = rest
                    .get(i)
                    .ok_or_else(|| "flag `-target` requires an argument".to_string())?;
                target = Some((*v).to_string());
            }
            "-h" | "--help" => return Ok(Command::Help),
            _ if a.starts_with('-') => {
                return Err(format!("unknown flag `{a}` for `build`"));
            }
            _ => {
                if file.is_some() {
                    return Err(format!("unexpected extra argument `{a}` for `build`"));
                }
                file = Some(a.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Build { file, out, target })
}

fn parse_run(rest: &[&str]) -> Result<Command, String> {
    let mut file: Option<String> = None;
    let mut prog_args: Vec<String> = Vec::new();
    let mut after_dd = false;

    for &a in rest {
        if after_dd {
            prog_args.push(a.to_string());
        } else if a == "--" {
            after_dd = true;
        } else if a == "-h" || a == "--help" {
            return Ok(Command::Help);
        } else if a.starts_with('-') {
            return Err(format!("unknown flag `{a}` for `run`"));
        } else if file.is_some() {
            return Err(format!(
                "unexpected extra argument `{a}` for `run`; use `--` to pass program arguments"
            ));
        } else {
            file = Some(a.to_string());
        }
    }

    Ok(Command::Run {
        file,
        args: prog_args,
    })
}

fn parse_test(rest: &[&str]) -> Result<Command, String> {
    let mut file: Option<String> = None;
    for &a in rest {
        if a == "-h" || a == "--help" {
            return Ok(Command::Help);
        }
        if a.starts_with('-') {
            return Err(format!("unknown flag `{a}` for `test`"));
        }
        if file.is_some() {
            return Err(format!("unexpected extra argument `{a}` for `test`"));
        }
        file = Some(a.to_string());
    }
    Ok(Command::Test { file })
}

fn parse_fmt(rest: &[&str]) -> Result<Command, String> {
    let mut file: Option<String> = None;
    let mut mode = FmtMode::Stdout;

    for &a in rest {
        match a {
            "--check" => {
                if mode == FmtMode::Write {
                    return Err("`--check` and `-w` cannot be combined".to_string());
                }
                mode = FmtMode::Check;
            }
            "-w" => {
                if mode == FmtMode::Check {
                    return Err("`--check` and `-w` cannot be combined".to_string());
                }
                mode = FmtMode::Write;
            }
            "-h" | "--help" => return Ok(Command::Help),
            _ if a.starts_with('-') => {
                return Err(format!("unknown flag `{a}` for `fmt`"));
            }
            _ => {
                if file.is_some() {
                    return Err(format!("unexpected extra argument `{a}` for `fmt`"));
                }
                file = Some(a.to_string());
            }
        }
    }

    match file {
        Some(f) => Ok(Command::Fmt { file: f, mode }),
        None => Err("`fmt` requires a FILE argument".to_string()),
    }
}

fn parse_init(rest: &[&str]) -> Result<Command, String> {
    let mut name: Option<String> = None;
    for &a in rest {
        if a == "-h" || a == "--help" {
            return Ok(Command::Help);
        }
        if a.starts_with('-') {
            return Err(format!("unknown flag `{a}` for `init`"));
        }
        if name.is_some() {
            return Err(format!("unexpected extra argument `{a}` for `init`"));
        }
        name = Some(a.to_string());
    }
    Ok(Command::Init { name })
}

// ---------------------------------------------------------------------------
// Source resolution & compilation (shared by build/run/test).
// ---------------------------------------------------------------------------

/// A resolved compilation input.
struct Source {
    /// The filename used to anchor rendered diagnostics.
    filename: String,
    /// The source text.
    text: String,
    /// The default output executable name for `build` (source minus `.ks`, or
    /// the `name` from `build.ks`).
    default_out: String,
}

/// Read a file as UTF-8, mapping I/O failure to a newline-terminated,
/// ready-to-print error message.
fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("error: cannot read `{path}`: {e}\n"))
}

/// Strip a trailing `.ks` to produce a default executable name. A path with no
/// `.ks` suffix (or exactly `.ks`) gets a `.out` suffix so the source is never
/// overwritten.
fn default_out_name(path: &str) -> String {
    match path.strip_suffix(".ks") {
        Some(stem) if !stem.is_empty() => stem.to_string(),
        _ => format!("{path}.out"),
    }
}

/// Resolve the source to compile. With an explicit `file`, read it directly.
/// Without one, read `./build.ks` for the `root` source and output `name`
/// (SPEC §6/§7). The error string is fully rendered and newline-terminated,
/// ready to print to stderr.
fn resolve_source(file: Option<&str>) -> Result<Source, String> {
    match file {
        Some(f) => {
            let text = read_file(f)?;
            Ok(Source {
                filename: f.to_string(),
                text,
                default_out: default_out_name(f),
            })
        }
        None => {
            let build_src = match read_file("build.ks") {
                Ok(s) => s,
                Err(e) => {
                    return Err(format!(
                        "{e}note: no FILE given, so `kard` looked for `./build.ks` in the current directory\n"
                    ));
                }
            };
            let spec = match crate::build_system::parse_build_kd(&build_src) {
                Ok(spec) => spec,
                Err(diags) => {
                    return Err(crate::diag::render_all(&diags, "build.ks", &build_src));
                }
            };
            let text = read_file(&spec.root)?;
            Ok(Source {
                filename: spec.root,
                text,
                default_out: spec.name,
            })
        }
    }
}

/// Resolve and compile a source for `mode`. On failure, the appropriate error
/// (rendered diagnostics or an I/O message) is already printed to stderr; the
/// caller just needs to return a failure exit code.
fn compile_source(file: Option<String>, mode: EmitMode) -> Result<(Source, String), ()> {
    let src = match resolve_source(file.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            eprint!("{msg}");
            return Err(());
        }
    };
    match crate::compile_to_c(&src.text, mode) {
        Ok(c) => Ok((src, c)),
        Err(diags) => {
            eprint!("{}", crate::diag::render_all(&diags, &src.filename, &src.text));
            Err(())
        }
    }
}

// ---------------------------------------------------------------------------
// Subcommand execution.
// ---------------------------------------------------------------------------

fn cmd_build(file: Option<String>, out: Option<String>, target: Option<String>) -> ExitCode {
    if let Some(t) = &target {
        eprintln!(
            "note: `-target {t}` is accepted, but the cross-compilation matrix is a roadmap item; building for the host target"
        );
    }

    let (src, c) = match compile_source(file, EmitMode::Program) {
        Ok(v) => v,
        Err(()) => return ExitCode::FAILURE,
    };

    let out_path = out.unwrap_or(src.default_out);
    match crate::backend::cc_build(&c, Path::new(&out_path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: C compilation failed:\n{e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_run(file: Option<String>, prog_args: Vec<String>) -> ExitCode {
    let (_src, c) = match compile_source(file, EmitMode::Program) {
        Ok(v) => v,
        Err(()) => return ExitCode::FAILURE,
    };

    match crate::backend::cc_build_and_run(&c, &prog_args) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_test(file: Option<String>) -> ExitCode {
    let (_src, c) = match compile_source(file, EmitMode::Test) {
        Ok(v) => v,
        Err(()) => return ExitCode::FAILURE,
    };

    // The harness itself prints per-test `ok:`/`FAIL:` lines and a final
    // `<passed>/<total> tests passed` summary to stderr, and exits with the
    // failure count. We add a one-line outcome summary on stdout.
    match crate::backend::cc_build_and_run(&c, &[]) {
        Ok(0) => {
            println!("all tests passed");
            ExitCode::SUCCESS
        }
        Ok(n) => {
            println!("{n} test(s) failed");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_fmt(file: String, mode: FmtMode) -> ExitCode {
    let src = match read_file(&file) {
        Ok(s) => s,
        Err(msg) => {
            eprint!("{msg}");
            return ExitCode::FAILURE;
        }
    };

    let formatted = match crate::fmt::format_source(&src) {
        Ok(f) => f,
        Err(diags) => {
            eprint!("{}", crate::diag::render_all(&diags, &file, &src));
            return ExitCode::FAILURE;
        }
    };

    match mode {
        FmtMode::Check => {
            if formatted == src {
                ExitCode::SUCCESS
            } else {
                eprintln!("{file}: not formatted (run `kard fmt -w {file}`)");
                ExitCode::FAILURE
            }
        }
        FmtMode::Write => {
            if formatted == src {
                // Already canonical; avoid a needless write (and mtime churn).
                ExitCode::SUCCESS
            } else {
                match std::fs::write(&file, formatted.as_bytes()) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("error: cannot write `{file}`: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
        }
        FmtMode::Stdout => {
            print!("{formatted}");
            ExitCode::SUCCESS
        }
    }
}

fn cmd_init(name: Option<String>) -> ExitCode {
    // `init NAME` scaffolds ./NAME named NAME; `init` (no NAME) scaffolds the
    // current directory, named after its basename (SPEC §6).
    let (dir, proj_name) = match name {
        // The directory is the given path; the project name is its basename, so
        // `init path/to/demo` produces a project named `demo` (not the path).
        Some(n) => {
            let base = Path::new(&n)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| n.clone());
            (n, base)
        }
        None => {
            let base = std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "kardashev_project".to_string());
            (".".to_string(), base)
        }
    };

    match crate::scaffold::init_project(Path::new(&dir), &proj_name) {
        Ok(()) => {
            println!("created kardashev project `{proj_name}` in `{dir}`");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an owned argument vector from string slices.
    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn parse(parts: &[&str]) -> Result<Command, String> {
        parse_args(&args(parts))
    }

    #[test]
    fn no_args_is_help() {
        assert_eq!(parse(&[]).unwrap(), Command::Help);
    }

    #[test]
    fn help_aliases() {
        for a in ["help", "--help", "-h"] {
            assert_eq!(parse(&[a]).unwrap(), Command::Help);
        }
    }

    #[test]
    fn version_aliases() {
        for a in ["version", "--version", "-V"] {
            assert_eq!(parse(&[a]).unwrap(), Command::Version);
        }
    }

    #[test]
    fn build_with_file() {
        assert_eq!(
            parse(&["build", "main.ks"]).unwrap(),
            Command::Build {
                file: Some("main.ks".to_string()),
                out: None,
                target: None,
            }
        );
    }

    #[test]
    fn build_with_no_file() {
        assert_eq!(
            parse(&["build"]).unwrap(),
            Command::Build {
                file: None,
                out: None,
                target: None,
            }
        );
    }

    #[test]
    fn build_with_out_and_target_in_any_order() {
        assert_eq!(
            parse(&["build", "-o", "prog", "-target", "x86_64-linux", "src.ks"]).unwrap(),
            Command::Build {
                file: Some("src.ks".to_string()),
                out: Some("prog".to_string()),
                target: Some("x86_64-linux".to_string()),
            }
        );
        // Flags may also follow the positional.
        assert_eq!(
            parse(&["build", "src.ks", "-o", "prog"]).unwrap(),
            Command::Build {
                file: Some("src.ks".to_string()),
                out: Some("prog".to_string()),
                target: None,
            }
        );
    }

    #[test]
    fn build_dash_o_requires_value() {
        assert!(parse(&["build", "-o"]).is_err());
        assert!(parse(&["build", "-target"]).is_err());
    }

    #[test]
    fn build_unknown_flag_and_extra_arg_error() {
        assert!(parse(&["build", "--zonk"]).is_err());
        assert!(parse(&["build", "a.ks", "b.ks"]).is_err());
    }

    #[test]
    fn run_collects_program_args_after_double_dash() {
        assert_eq!(
            parse(&["run", "main.ks", "--", "alpha", "-V", "beta"]).unwrap(),
            Command::Run {
                file: Some("main.ks".to_string()),
                args: vec!["alpha".to_string(), "-V".to_string(), "beta".to_string()],
            }
        );
    }

    #[test]
    fn run_with_no_file_and_no_args() {
        assert_eq!(
            parse(&["run"]).unwrap(),
            Command::Run {
                file: None,
                args: vec![],
            }
        );
    }

    #[test]
    fn run_double_dash_with_no_file() {
        assert_eq!(
            parse(&["run", "--", "x"]).unwrap(),
            Command::Run {
                file: None,
                args: vec!["x".to_string()],
            }
        );
    }

    #[test]
    fn test_with_and_without_file() {
        assert_eq!(
            parse(&["test", "t.ks"]).unwrap(),
            Command::Test {
                file: Some("t.ks".to_string()),
            }
        );
        assert_eq!(parse(&["test"]).unwrap(), Command::Test { file: None });
    }

    #[test]
    fn fmt_modes() {
        assert_eq!(
            parse(&["fmt", "f.ks"]).unwrap(),
            Command::Fmt {
                file: "f.ks".to_string(),
                mode: FmtMode::Stdout,
            }
        );
        assert_eq!(
            parse(&["fmt", "f.ks", "--check"]).unwrap(),
            Command::Fmt {
                file: "f.ks".to_string(),
                mode: FmtMode::Check,
            }
        );
        assert_eq!(
            parse(&["fmt", "-w", "f.ks"]).unwrap(),
            Command::Fmt {
                file: "f.ks".to_string(),
                mode: FmtMode::Write,
            }
        );
    }

    #[test]
    fn fmt_requires_a_file() {
        assert!(parse(&["fmt"]).is_err());
        assert!(parse(&["fmt", "--check"]).is_err());
    }

    #[test]
    fn fmt_conflicting_modes_error() {
        assert!(parse(&["fmt", "f.ks", "--check", "-w"]).is_err());
        assert!(parse(&["fmt", "f.ks", "-w", "--check"]).is_err());
    }

    #[test]
    fn init_with_and_without_name() {
        assert_eq!(
            parse(&["init", "demo"]).unwrap(),
            Command::Init {
                name: Some("demo".to_string()),
            }
        );
        assert_eq!(parse(&["init"]).unwrap(), Command::Init { name: None });
    }

    #[test]
    fn unknown_subcommand_errors() {
        assert!(parse(&["frobnicate"]).is_err());
    }

    #[test]
    fn default_out_name_strips_kd() {
        assert_eq!(default_out_name("main.ks"), "main");
        assert_eq!(default_out_name("src/app.ks"), "src/app");
        // No `.ks` suffix: append `.out` rather than clobber the source.
        assert_eq!(default_out_name("prog"), "prog.out");
        assert_eq!(default_out_name(".ks"), ".ks.out");
    }
}
