//! Command-line dispatch for the `kard` toolchain binary.
//!
//! A single binary that is compiler, build system, test runner and formatter
//! (SPEC §6). Argument parsing is hand-rolled — no external crates — and split
//! into a pure [`parse_args`] step (returning a [`Command`]) and an execution
//! step (one `cmd_*` function per subcommand). The split keeps argument parsing
//! unit-testable without ever invoking the C compiler.
//!
//! Subcommands: `build`, `run`, `test`, `fmt`, `init`, `targets`, `version`,
//! `help`.

use std::path::Path;
use std::process::ExitCode;

use crate::build_system::{BuildSpec, Target};
use crate::emit_c::EmitMode;

/// Top-level usage text, printed by `help` (to stdout) and after a usage error
/// (to stderr).
const USAGE: &str = "\
kardashev — a self-contained toolchain for the kardashev systems language.

Usage:
    kard <command> [options]

Commands:
    build [FILE|TARGET] [-o OUT] [-target TRIPLE] [-c | --emit obj]
            Compile a program to a native executable. A `.ks` (or existing)
            argument is a source FILE; any other argument names a build.ks
            TARGET. With no argument, reads ./build.ks: a single target is
            built, and a multi-target graph builds every target (each to its
            own name). OUT defaults to the target name (or, for a direct FILE,
            the filename without its `.ks` extension).
            `-target TRIPLE` cross-compiles via the C compiler's `--target=`
            (clang). `-c` / `--emit obj` emits an object file only (no link),
            which cross-compiles without a target sysroot; its default OUT is
            the source/target name with a `.o` extension. See `targets`.

    run   [FILE|TARGET] [-- ARGS...]
            Build to a temporary executable, run it, and propagate its exit
            code. Arguments after `--` are passed through to the program. With
            no argument and multiple targets in ./build.ks, a TARGET name is
            required.

    test  [FILE|TARGET]
            Build and run the test harness; reports pass/fail counts and
            exits non-zero if any test fails. With no argument and multiple
            targets in ./build.ks, a TARGET name is required.

    fmt   FILE [--check | -w]
            Format source. With no flag, prints canonical source to stdout.
            --check exits non-zero if FILE is not already canonical; -w
            rewrites FILE in place.

    doc   FILE
            Print Markdown API documentation for FILE's public items, using the
            `///` doc comments written above each one.

    init  [NAME]
            Scaffold a new project. With NAME, creates ./NAME; otherwise
            scaffolds into the current directory.

    targets
            List common, known-good cross-compilation target triples (for
            use with `build -target TRIPLE`).

    version
            Print the toolchain version. (also --version, -V)

    help
            Print this help. (also --help, -h)
";

/// A parsed, validated command line. Produced by [`parse_args`] and consumed by
/// [`run`]; carrying no I/O lets the parser be tested in isolation.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Command {
    /// `build [FILE] [-o OUT] [-target TRIPLE] [-c | --emit obj]`
    Build {
        file: Option<String>,
        out: Option<String>,
        target: Option<String>,
        /// `-c` / `--emit obj`: compile to an object file only (no link).
        object_only: bool,
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
    /// `doc FILE` — print Markdown API docs for a file's public items (v0.140).
    Doc { file: String },
    /// `init [NAME]`
    Init { name: Option<String> },
    /// `targets` — list known cross-compilation target triples.
    Targets,
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
        Command::Build {
            file,
            out,
            target,
            object_only,
        } => cmd_build(file, out, target, object_only),
        Command::Run { file, args } => cmd_run(file, args),
        Command::Test { file } => cmd_test(file),
        Command::Fmt { file, mode } => cmd_fmt(file, mode),
        Command::Doc { file } => cmd_doc(file),
        Command::Init { name } => cmd_init(name),
        Command::Targets => cmd_targets(),
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
        "doc" => parse_doc(&rest),
        "init" => parse_init(&rest),
        "targets" => parse_targets(&rest),
        "version" | "--version" | "-V" => Ok(Command::Version),
        "help" | "--help" | "-h" => Ok(Command::Help),
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

fn parse_build(rest: &[&str]) -> Result<Command, String> {
    let mut file: Option<String> = None;
    let mut out: Option<String> = None;
    let mut target: Option<String> = None;
    let mut object_only = false;

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
            "-c" => {
                object_only = true;
            }
            "--emit" => {
                i += 1;
                let v = rest
                    .get(i)
                    .ok_or_else(|| "flag `--emit` requires an argument (`obj`)".to_string())?;
                match *v {
                    "obj" => object_only = true,
                    other => {
                        return Err(format!(
                            "unknown `--emit` mode `{other}` for `build` (expected `obj`)"
                        ));
                    }
                }
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

    Ok(Command::Build {
        file,
        out,
        target,
        object_only,
    })
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

fn parse_doc(rest: &[&str]) -> Result<Command, String> {
    let mut file: Option<String> = None;
    for &a in rest {
        match a {
            "-h" | "--help" => return Ok(Command::Help),
            _ if a.starts_with('-') => return Err(format!("unknown flag `{a}` for `doc`")),
            _ => {
                if file.is_some() {
                    return Err(format!("unexpected extra argument `{a}` for `doc`"));
                }
                file = Some(a.to_string());
            }
        }
    }
    match file {
        Some(f) => Ok(Command::Doc { file: f }),
        None => Err("`doc` requires a FILE argument".to_string()),
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

fn parse_targets(rest: &[&str]) -> Result<Command, String> {
    for &a in rest {
        if a == "-h" || a == "--help" {
            return Ok(Command::Help);
        }
        return Err(format!("unexpected argument `{a}` for `targets`"));
    }
    Ok(Command::Targets)
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

/// Whether a positional `build`/`run`/`test` argument names a source FILE
/// directly (it ends in `.ks`) as opposed to a build-graph TARGET (SPEC §7).
///
/// We key purely on the `.ks` extension — not on whether a file of that name
/// exists — so that a target name (e.g. `app`) is never misread as a file just
/// because a previous build left an executable of the same name in the cwd.
fn looks_like_file(arg: &str) -> bool {
    arg.ends_with(".ks")
}

/// A resolved compilation input built directly from a `.ks` source file.
fn source_from_file(path: &str) -> Result<Source, String> {
    let text = read_file(path)?;
    Ok(Source {
        filename: path.to_string(),
        text,
        default_out: default_out_name(path),
    })
}

/// A resolved compilation input built from a build-graph target: the source is
/// the target's `root` and the default output name is the target's `name`.
fn source_from_target(target: &Target) -> Result<Source, String> {
    let text = read_file(&target.root)?;
    Ok(Source {
        filename: target.root.clone(),
        text,
        default_out: target.name.clone(),
    })
}

/// Read & parse `./build.ks` into a [`BuildSpec`]. On a missing file,
/// `missing_note` (a fully rendered, newline-terminated note) is appended to the
/// I/O error; on a parse failure, the `E0300` diagnostics are rendered. The
/// returned error string is ready to print to stderr.
fn read_build_spec(missing_note: &str) -> Result<BuildSpec, String> {
    let build_src = match read_file("build.ks") {
        Ok(s) => s,
        Err(e) => return Err(format!("{e}{missing_note}")),
    };
    crate::build_system::parse_build_kd(&build_src)
        .map_err(|diags| crate::diag::render_all(&diags, "build.ks", &build_src))
}

/// An "available targets: a, b, c" hint, or empty when there are none.
fn target_list_hint(spec: &BuildSpec) -> String {
    if spec.targets.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = spec.targets.iter().map(|t| t.name.as_str()).collect();
    format!("\nnote: available targets: {}", names.join(", "))
}

/// Select the build-graph target named `name`, or a ready-to-print error with an
/// "available targets" hint. Pure (no I/O), so it is unit-testable.
fn select_target<'a>(spec: &'a BuildSpec, name: &str) -> Result<&'a Target, String> {
    spec.select(Some(name)).ok_or_else(|| {
        format!(
            "error: no target named `{name}` in `build.ks`{}\n",
            target_list_hint(spec)
        )
    })
}

/// Select the sole target (when `build.ks` declares exactly one), or a
/// ready-to-print error: multiple targets require a name, zero is malformed.
/// Pure (no I/O), so it is unit-testable.
fn select_sole_target(spec: &BuildSpec) -> Result<&Target, String> {
    match spec.select(None) {
        Some(t) => Ok(t),
        None if spec.targets.is_empty() => {
            Err("error: `build.ks` declares no targets\n".to_string())
        }
        None => Err(format!(
            "error: `build.ks` declares multiple targets; specify one{}\n",
            target_list_hint(spec)
        )),
    }
}

/// Resolve the single source to compile for `run`/`test`. A `.ks` (or existing)
/// positional is a direct FILE; any other positional is a build-graph TARGET
/// name; with no positional, the sole target of `./build.ks` is used (multiple
/// targets are an error, since `run`/`test` act on exactly one program). The
/// error string is fully rendered and newline-terminated.
fn resolve_single_source(file: Option<&str>) -> Result<Source, String> {
    match file {
        Some(arg) if looks_like_file(arg) => source_from_file(arg),
        Some(name) => {
            let spec = read_build_spec(&target_lookup_note(name))?;
            source_from_target(select_target(&spec, name)?)
        }
        None => {
            let spec = read_build_spec(NO_FILE_NOTE)?;
            source_from_target(select_sole_target(&spec)?)
        }
    }
}

/// Resolve the source(s) to compile for `build`. Like [`resolve_single_source`],
/// except that with no positional and *multiple* targets, **all** targets are
/// built (each to its own name) rather than being an error.
fn resolve_build_sources(file: Option<&str>) -> Result<Vec<Source>, String> {
    match file {
        Some(arg) if looks_like_file(arg) => Ok(vec![source_from_file(arg)?]),
        Some(name) => {
            let spec = read_build_spec(&target_lookup_note(name))?;
            Ok(vec![source_from_target(select_target(&spec, name)?)?])
        }
        None => {
            let spec = read_build_spec(NO_FILE_NOTE)?;
            if spec.targets.is_empty() {
                return Err("error: `build.ks` declares no targets\n".to_string());
            }
            // One target, or the whole graph: build them all, each to its name.
            spec.targets.iter().map(source_from_target).collect()
        }
    }
}

/// The note appended when `./build.ks` is missing and no positional was given.
const NO_FILE_NOTE: &str =
    "note: no FILE given, so `kard` looked for `./build.ks` in the current directory\n";

/// The note appended when a non-`.ks` positional was treated as a TARGET name
/// but `./build.ks` is missing.
fn target_lookup_note(name: &str) -> String {
    format!(
        "note: `{name}` is not a `.ks` file, so `kard` treated it as a build-graph target \
         and looked for `./build.ks` in the current directory\n"
    )
}

/// Lower one resolved [`Source`] to C, printing rendered diagnostics to stderr
/// on failure.
///
/// Compilation is **path-based** ([`crate::compile_program`]) rather than
/// text-based: the source's filename anchors the flattener so that any
/// `@import("…")` declarations are resolved relative to it and the whole
/// program is concatenated into one module before sema/emit (SPEC §22.3). For a
/// direct `.ks` FILE the filename is that path; for a `build.ks` TARGET it is
/// the target's `root`.
///
/// Diagnostics are rendered against the root source text for display, mirroring
/// the previous single-file error path. A flattener `E0294` diagnostic already
/// carries a pre-rendered sub-file block in its message, so rendering it against
/// the root source is acceptable for v0.126.
fn compile_one(src: &Source, mode: EmitMode) -> Result<String, ()> {
    crate::compile_program(Path::new(&src.filename), mode)
        .map_err(|diags| eprint!("{}", crate::diag::render_all(&diags, &src.filename, &src.text)))
}

/// Resolve and compile a single source for `run`/`test`. On failure, the
/// appropriate error (rendered diagnostics or an I/O message) is already printed
/// to stderr; the caller just needs to return a failure exit code.
fn compile_source(file: Option<String>, mode: EmitMode) -> Result<(Source, String), ()> {
    let src = match resolve_single_source(file.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            eprint!("{msg}");
            return Err(());
        }
    };
    let c = compile_one(&src, mode)?;
    Ok((src, c))
}

// ---------------------------------------------------------------------------
// Subcommand execution.
// ---------------------------------------------------------------------------

fn cmd_build(
    file: Option<String>,
    out: Option<String>,
    target: Option<String>,
    object_only: bool,
) -> ExitCode {
    let sources = match resolve_build_sources(file.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            eprint!("{msg}");
            return ExitCode::FAILURE;
        }
    };

    // `-o OUT` names one output file; it is meaningless when building a whole
    // multi-target graph, where each target compiles to its own name.
    if out.is_some() && sources.len() > 1 {
        eprintln!(
            "error: `-o OUT` cannot be used when building multiple targets; build a single target by name"
        );
        return ExitCode::FAILURE;
    }

    // Cross-compilation / object-emit options are uniform across every target
    // in the build, so the backend options are built once.
    let opts = crate::backend::BuildOptions {
        target,
        object_only,
    };

    for src in &sources {
        let c = match compile_one(src, EmitMode::Program) {
            Ok(c) => c,
            Err(()) => return ExitCode::FAILURE,
        };
        // In object mode the default OUT is the program name with a `.o`
        // extension (SPEC §19); an explicit `-o OUT` always wins.
        let out_path = out
            .clone()
            .unwrap_or_else(|| default_build_out(&src.default_out, object_only));
        if let Err(e) = crate::backend::cc_build(&c, Path::new(&out_path), &opts) {
            eprintln!("error: C compilation failed:\n{e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// The default output path for a `build`: the executable name as-is, or, in
/// object-only mode (`-c` / `--emit obj`), that name with a `.o` extension.
fn default_build_out(default_out: &str, object_only: bool) -> String {
    if object_only {
        format!("{default_out}.o")
    } else {
        default_out.to_string()
    }
}

/// Print every known cross-compilation target triple, one per line (SPEC §19).
fn cmd_targets() -> ExitCode {
    for triple in crate::backend::known_targets() {
        println!("{triple}");
    }
    ExitCode::SUCCESS
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

/// `kard doc FILE` — render Markdown API documentation for a file's public
/// items and their `///` doc comments (v0.140). Doc comments are ordinary
/// (ignored) `//` comments to the compiler; this command associates the
/// contiguous `///` lines directly above each `pub` item by source position.
fn cmd_doc(file: String) -> ExitCode {
    let src = match read_file(&file) {
        Ok(s) => s,
        Err(msg) => {
            eprint!("{msg}");
            return ExitCode::FAILURE;
        }
    };
    let tokens = match crate::lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => {
            eprint!("{}", crate::diag::render_all(&diags, &file, &src));
            return ExitCode::FAILURE;
        }
    };
    let module = match crate::parser::parse(&tokens) {
        Ok(m) => m,
        Err(diags) => {
            eprint!("{}", crate::diag::render_all(&diags, &file, &src));
            return ExitCode::FAILURE;
        }
    };
    print!("{}", render_docs(&file, &src, &module));
    ExitCode::SUCCESS
}

/// Byte offset of the start of each line in `src`.
fn line_starts(src: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

/// The contiguous `///` doc-comment lines immediately above byte offset `at`,
/// in source order, with the `///` (and one optional space) stripped.
fn doc_above(src: &str, starts: &[usize], at: usize) -> Vec<String> {
    let line = match starts.binary_search(&at) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let mut docs: Vec<String> = Vec::new();
    let mut i = line;
    while i > 0 {
        i -= 1;
        let s = starts[i];
        let e = if i + 1 < starts.len() { starts[i + 1] } else { src.len() };
        let text = src[s..e].trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = text.trim_start();
        if let Some(rest) = trimmed.strip_prefix("///") {
            docs.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else {
            break;
        }
    }
    docs.reverse();
    docs
}

/// A compact source-like spelling of a type for a doc signature.
fn doc_type_str(te: &crate::ast::TypeExpr) -> String {
    use crate::ast::ArraySize;
    if let Some(sz) = &te.array_len {
        let n = match sz {
            ArraySize::Lit(n) => n.to_string(),
            ArraySize::Param(s) => s.clone(),
        };
        return format!("[{}]{}", n, te.name);
    }
    if te.optional {
        return format!("?{}", te.name);
    }
    if te.error_union {
        return match &te.error_set {
            Some(s) => format!("{}!{}", s, te.name),
            None => format!("!{}", te.name),
        };
    }
    if te.pointer {
        return format!("*{}", te.name);
    }
    if te.slice {
        return format!("[]{}", te.name);
    }
    te.name.clone()
}

/// A one-line signature for a function, e.g. `fn add(a: i32, b: i32) i32`.
fn doc_func_sig(f: &crate::ast::Func) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| {
            let pre = if p.is_comptime { "comptime " } else { "" };
            format!("{}{}: {}", pre, p.name, doc_type_str(&p.ty))
        })
        .collect();
    format!("fn {}({}) {}", f.name, params.join(", "), doc_type_str(&f.ret))
}

/// Render a module's public API as Markdown.
fn render_docs(file: &str, src: &str, module: &crate::ast::Module) -> String {
    use crate::ast::Item;
    let starts = line_starts(src);
    let mut out = format!("# `{}`\n\n", file);
    let mut any = false;
    for item in &module.items {
        let (sig, span) = match item {
            Item::Func(f) if f.is_pub => (doc_func_sig(f), f.span),
            Item::Const(c) if c.is_pub => {
                let ty = match &c.ty {
                    Some(t) => format!(": {}", doc_type_str(t)),
                    None => String::new(),
                };
                (format!("const {}{}", c.name, ty), c.span)
            }
            Item::Struct(s) if s.is_pub => (format!("struct {}", s.name), s.span),
            Item::Enum(e) if e.is_pub => (format!("enum {}", e.name), e.span),
            Item::Union(u) if u.is_pub => (format!("union {}", u.name), u.span),
            Item::ErrorSet(e) if e.is_pub => (format!("error set {}", e.name), e.span),
            _ => continue,
        };
        any = true;
        out.push_str(&format!("## `{}`\n\n", sig));
        let doc = doc_above(src, &starts, span.start);
        if doc.is_empty() {
            out.push_str("_No documentation._\n\n");
        } else {
            for line in &doc {
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
    }
    if !any {
        out.push_str("_No public items._\n");
    }
    out
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
                object_only: false,
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
                object_only: false,
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
                object_only: false,
            }
        );
        // Flags may also follow the positional.
        assert_eq!(
            parse(&["build", "src.ks", "-o", "prog"]).unwrap(),
            Command::Build {
                file: Some("src.ks".to_string()),
                out: Some("prog".to_string()),
                target: None,
                object_only: false,
            }
        );
    }

    #[test]
    fn build_target_is_captured() {
        assert_eq!(
            parse(&["build", "f.ks", "-target", "aarch64-linux-gnu"]).unwrap(),
            Command::Build {
                file: Some("f.ks".to_string()),
                out: None,
                target: Some("aarch64-linux-gnu".to_string()),
                object_only: false,
            }
        );
    }

    #[test]
    fn build_dash_c_sets_object_only() {
        assert_eq!(
            parse(&["build", "f.ks", "-c"]).unwrap(),
            Command::Build {
                file: Some("f.ks".to_string()),
                out: None,
                target: None,
                object_only: true,
            }
        );
    }

    #[test]
    fn build_emit_obj_sets_object_only() {
        assert_eq!(
            parse(&["build", "--emit", "obj", "f.ks"]).unwrap(),
            Command::Build {
                file: Some("f.ks".to_string()),
                out: None,
                target: None,
                object_only: true,
            }
        );
    }

    #[test]
    fn build_object_only_with_target_and_out() {
        assert_eq!(
            parse(&["build", "f.ks", "-target", "aarch64-linux-gnu", "-c", "-o", "f.o"]).unwrap(),
            Command::Build {
                file: Some("f.ks".to_string()),
                out: Some("f.o".to_string()),
                target: Some("aarch64-linux-gnu".to_string()),
                object_only: true,
            }
        );
    }

    #[test]
    fn build_emit_requires_value_and_rejects_unknown() {
        assert!(parse(&["build", "--emit"]).is_err());
        assert!(parse(&["build", "--emit", "asm"]).is_err());
    }

    #[test]
    fn default_build_out_appends_o_only_in_object_mode() {
        assert_eq!(default_build_out("main", false), "main");
        assert_eq!(default_build_out("main", true), "main.o");
        // Build-graph target names (no `.ks` to strip) get a plain `.o`.
        assert_eq!(default_build_out("app", true), "app.o");
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
    fn doc_takes_one_file() {
        assert_eq!(
            parse(&["doc", "lib.ks"]).unwrap(),
            Command::Doc {
                file: "lib.ks".to_string()
            }
        );
        assert!(parse(&["doc"]).is_err()); // FILE required
        assert!(parse(&["doc", "a.ks", "b.ks"]).is_err()); // one file only
        assert!(parse(&["doc", "--zonk"]).is_err()); // unknown flag
    }

    #[test]
    fn doc_renders_pub_items_with_their_doc_comments() {
        let src = "\
/// The answer.
pub const ANSWER: i32 = 42;

fn hidden() i32 { return 0; }

/// Doubles its input.
pub fn twice(n: i32) i32 { return n + n; }
";
        let module = crate::parser::parse(&crate::lexer::lex(src).unwrap()).unwrap();
        let md = render_docs("lib.ks", src, &module);
        assert!(md.contains("# `lib.ks`"));
        assert!(md.contains("## `const ANSWER: i32`"));
        assert!(md.contains("The answer."));
        assert!(md.contains("## `fn twice(n: i32) i32`"));
        assert!(md.contains("Doubles its input."));
        // A non-`pub` item is omitted from the public API docs.
        assert!(!md.contains("hidden"));
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
    fn targets_parses() {
        assert_eq!(parse(&["targets"]).unwrap(), Command::Targets);
        // `-h`/`--help` short-circuit to Help, like the other subcommands.
        assert_eq!(parse(&["targets", "--help"]).unwrap(), Command::Help);
        // Stray positional arguments are rejected.
        assert!(parse(&["targets", "x86_64-linux"]).is_err());
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

    // ---- Build-graph target selection (v0.122) --------------------------
    //
    // These exercise the pure selection helpers with constructed specs; none
    // touch the filesystem or invoke the C compiler.

    fn mk_spec(targets: &[(&str, &str)]) -> BuildSpec {
        BuildSpec {
            targets: targets
                .iter()
                .map(|(n, r)| Target {
                    name: (*n).to_string(),
                    root: (*r).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn looks_like_file_distinguishes_files_from_target_names() {
        // A `.ks` suffix always reads as a direct FILE.
        assert!(looks_like_file("main.ks"));
        assert!(looks_like_file("src/app.ks"));
        // Names without a `.ks` suffix (and not naming an existing file) are
        // treated as build-graph TARGET names.
        assert!(!looks_like_file("app"));
        assert!(!looks_like_file("a-target-name-that-does-not-exist"));
    }

    #[test]
    fn select_target_finds_named_target() {
        let spec = mk_spec(&[("app", "src/main.ks"), ("tool", "src/tool.ks")]);
        assert_eq!(select_target(&spec, "tool").unwrap().root, "src/tool.ks");
        assert_eq!(select_target(&spec, "app").unwrap().root, "src/main.ks");
    }

    #[test]
    fn select_target_unknown_errors_with_available_hint() {
        let spec = mk_spec(&[("app", "a.ks"), ("tool", "t.ks")]);
        let err = select_target(&spec, "nope").unwrap_err();
        assert!(err.contains("no target named `nope`"), "{err}");
        // The hint lists the real targets so the user can fix the typo.
        assert!(err.contains("app") && err.contains("tool"), "{err}");
    }

    #[test]
    fn select_sole_target_when_single() {
        let spec = mk_spec(&[("only", "m.ks")]);
        assert_eq!(select_sole_target(&spec).unwrap().name, "only");
    }

    #[test]
    fn select_sole_target_multiple_requires_a_name() {
        let spec = mk_spec(&[("a", "a.ks"), ("b", "b.ks")]);
        let err = select_sole_target(&spec).unwrap_err();
        assert!(err.contains("multiple targets"), "{err}");
        assert!(err.contains("specify one"), "{err}");
    }

    #[test]
    fn select_sole_target_zero_is_malformed() {
        let spec = mk_spec(&[]);
        let err = select_sole_target(&spec).unwrap_err();
        assert!(err.contains("no targets"), "{err}");
    }

    #[test]
    fn target_list_hint_lists_names_and_is_empty_when_none() {
        let spec = mk_spec(&[("a", "a.ks"), ("b", "b.ks")]);
        let hint = target_list_hint(&spec);
        assert!(hint.contains("available targets"), "{hint}");
        assert!(hint.contains('a') && hint.contains('b'), "{hint}");
        assert_eq!(target_list_hint(&mk_spec(&[])), "");
    }
}
