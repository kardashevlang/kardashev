//! Native backend driver: C source → object/executable via the system C
//! compiler, plus execution.
//!
//! This module owns *only* the cc invocation and process execution. The
//! lex→parse→sema→emit pipeline lives in [`crate::compile_to_c`].
//!
//! v0.123 adds cross-compilation: [`BuildOptions`] threads a target triple and
//! an object-only flag into [`cc_build`]. Cross-compilation leans on clang's
//! `--target=<triple>`; `-c` (object only) skips the link step so a foreign
//! object can be produced without a target sysroot/libc.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// C-compiler optimization level for a build.
///
/// The default is [`OptLevel::O2`], so `BuildOptions::default()` keeps the
/// historical optimized behavior for `kard build`, `kard bench` and
/// cross-compiles. `kard run`/`kard test` build unoptimized dev binaries at
/// [`OptLevel::O0`] for fast iteration (`--release` restores `-O2`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OptLevel {
    /// `-O0`: a fast, unoptimized dev build (`kard run`/`test` default).
    O0,
    /// `-O2`: an optimized build (`kard build`/`bench`/`--release`).
    #[default]
    O2,
}

/// Options for a C-backend build (v0.123 cross-compilation).
#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    /// A target triple for cross-compilation, passed to the C compiler as
    /// `--target=<triple>` (clang). `None` builds for the host.
    pub target: Option<String>,
    /// Compile to an object file only (`-c`), skipping the link step — which
    /// lets cross-compilation succeed without a target sysroot/libc.
    pub object_only: bool,
    /// Optimization level handed to the C compiler. Defaults to `-O2`.
    pub opt: OptLevel,
}

/// Monotonic counter giving each temp path a process-unique suffix even when
/// several builds run concurrently (e.g. parallel `cargo test` threads, which
/// share a PID).
static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Common, known-good target triples for `kard targets` (informational).
///
/// These are the triples we expect clang to accept for cross object emission;
/// a fully *linked* foreign executable additionally needs that target's C
/// toolchain/sysroot installed (see SPEC §19).
pub fn known_targets() -> &'static [&'static str] {
    &[
        "x86_64-linux-gnu",
        "aarch64-linux-gnu",
        "x86_64-apple-darwin",
        "arm64-apple-darwin",
        "wasm32-wasi",
        "x86_64-pc-windows-gnu",
    ]
}

/// True if `cc` names a clang-family compiler (its file name contains
/// `clang`), e.g. `clang`, `clang-17`, `/usr/bin/clang`. gcc/cc are not.
fn is_clang_like(cc: &str) -> bool {
    Path::new(cc)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.contains("clang"))
        .unwrap_or(false)
}

/// True if `cc --version` runs successfully (i.e. the compiler exists and is
/// invokable).
fn compiler_runs(cc: &str) -> bool {
    Command::new(cc)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the C compiler for a **host** build.
///
/// Honors `$CC` verbatim when it is set and non-empty; otherwise probes `cc`,
/// `clang` and `gcc` in order and returns the first whose `--version` runs
/// successfully.
fn discover_cc() -> Result<String, String> {
    if let Ok(cc) = std::env::var("CC") {
        if !cc.trim().is_empty() {
            return Ok(cc);
        }
    }
    for cand in ["cc", "clang", "gcc"] {
        if compiler_runs(cand) {
            return Ok(cand.to_string());
        }
    }
    Err("no C compiler found (tried $CC, then `cc`, `clang`, `gcc`)".to_string())
}

/// Resolve a **clang-family** compiler for a cross build.
///
/// Cross-compilation needs clang's `--target=` (gcc's `-target` is not
/// equivalent — gcc must be a separately-built cross toolchain). Honors `$CC`
/// only when it is itself clang-like; otherwise probes for `clang`. Returns a
/// clear, actionable error when no clang is available.
fn discover_clang() -> Result<String, String> {
    if let Ok(cc) = std::env::var("CC") {
        let cc = cc.trim();
        if !cc.is_empty() && is_clang_like(cc) && compiler_runs(cc) {
            return Ok(cc.to_string());
        }
    }
    if compiler_runs("clang") {
        return Ok("clang".to_string());
    }
    Err("cross-compilation (`-target`) requires a clang-family compiler, but \
         none was found (checked $CC and `clang`). gcc's `-target` is not \
         equivalent. Install clang, or use `-c` to emit a host object."
        .to_string())
}

/// Build a process-unique path under the system temp directory. `suffix` is
/// appended verbatim (e.g. `".c"` for sources, `".o"` for objects, `""` for
/// executables).
fn unique_temp(suffix: &str) -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("kardc_{}_{}{}", std::process::id(), n, suffix));
    path
}

/// Invoke the C compiler as
/// `<cc> <-O0|-O2> -ffp-contract=off -std=c11 [--target=<triple>] [-c] -o
/// <out> <tmp_c>`, returning the compiler's stderr on a non-zero exit. The
/// optimization level comes from `opts.opt`: `-O2` by default
/// (`build`/`bench`/cross-compiles), `-O0` for the `run`/`test` dev builds
/// (unless `--release`).
///
/// `-ffp-contract=off` pins **deterministic IEEE-754 float semantics** across
/// platforms (SPEC §38): Apple clang defaults to contracting `a * b + c` into
/// a fused multiply-add (one rounding instead of two), which made the same
/// kardashev program print different `f64` digits on macOS than on Linux
/// (found by the v0.157 std suite — `fmt_f64(0.1, 17)` differed in its last
/// digit). Both gcc and clang accept the flag.
fn invoke_cc(cc: &str, tmp_c: &Path, out: &Path, opts: &BuildOptions) -> Result<(), String> {
    let mut cmd = Command::new(cc);
    cmd.arg(match opts.opt {
        OptLevel::O0 => "-O0",
        OptLevel::O2 => "-O2",
    })
    .arg("-ffp-contract=off")
    .arg("-std=c11");
    if let Some(triple) = &opts.target {
        cmd.arg(format!("--target={triple}"));
    }
    if opts.object_only {
        // Compile only; do not link. Lets cross emission succeed without a
        // target sysroot/libc.
        cmd.arg("-c");
    }
    cmd.arg("-o").arg(out).arg(tmp_c);
    let output = cmd
        .output()
        .map_err(|e| format!("failed to invoke C compiler `{cc}`: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// Compile C source text to `out` using the system C compiler, honoring
/// [`BuildOptions`]. By default this links a native host executable; with
/// `opts.object_only` it emits an object file, and with `opts.target` it
/// selects clang and cross-compiles via `--target=<triple>`. Returns the
/// compiler's stderr on failure.
pub fn cc_build(c_src: &str, out: &Path, opts: &BuildOptions) -> Result<(), String> {
    // A target triple demands clang specifically; a plain host build accepts
    // any of cc/clang/gcc.
    let cc = if opts.target.is_some() {
        discover_clang()?
    } else {
        discover_cc()?
    };
    let tmp_c = unique_temp(".c");
    std::fs::write(&tmp_c, c_src)
        .map_err(|e| format!("failed to write temp C file {}: {e}", tmp_c.display()))?;
    let result = invoke_cc(&cc, &tmp_c, out, opts);
    // Best-effort cleanup of the temporary source regardless of outcome.
    let _ = std::fs::remove_file(&tmp_c);
    result
}

/// Run a compiled executable with `args`, translating its termination into an
/// exit code: a normal exit yields its status code; termination by signal
/// yields `128 + signal` (shell convention).
fn run_exe(exe: &Path, args: &[String]) -> Result<i32, String> {
    use std::os::unix::process::ExitStatusExt;
    let status = Command::new(exe)
        .args(args)
        .status()
        .map_err(|e| format!("failed to run compiled program {}: {e}", exe.display()))?;
    if let Some(code) = status.code() {
        Ok(code)
    } else if let Some(sig) = status.signal() {
        Ok(128 + sig)
    } else {
        Err("compiled program terminated abnormally".to_string())
    }
}

/// Compile C source text to a temporary **host executable** at `opt`, run it,
/// and return the child process exit code. Returns an error string on a
/// compile failure. `run`/`test` pass [`OptLevel::O0`] (a fast dev build);
/// `bench` and `--release` pass [`OptLevel::O2`].
pub fn cc_build_and_run(c_src: &str, args: &[String], opt: OptLevel) -> Result<i32, String> {
    let exe = unique_temp("");
    // Always build for the host as a linked executable: `run`/`test` never
    // cross-compile.
    cc_build(
        c_src,
        &exe,
        &BuildOptions {
            opt,
            ..BuildOptions::default()
        },
    )?;
    let result = run_exe(&exe, args);
    // Best-effort cleanup of the temporary executable.
    let _ = std::fs::remove_file(&exe);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_returns_exit_code() {
        // `-O0` is the `kard run`/`test` dev default.
        let code = cc_build_and_run("int main(){return 7;}", &[], OptLevel::O0)
            .expect("should compile and run");
        assert_eq!(code, 7);
    }

    #[test]
    fn run_program_that_prints() {
        // `-O2` covers the `bench`/`--release` path.
        let src = "#include <stdio.h>\nint main(void){ printf(\"hello from kardc\\n\"); return 0; }";
        let code = cc_build_and_run(src, &[], OptLevel::O2).expect("should compile and run");
        assert_eq!(code, 0);
    }

    #[test]
    fn broken_program_errs() {
        let result = cc_build_and_run("int main(){ this is not valid C @@@ }", &[], OptLevel::O0);
        assert!(result.is_err(), "broken C should yield Err, got {result:?}");
    }

    #[test]
    fn build_options_default_opt_is_o2() {
        // `kard build`, cross-compiles and the e2e suite all rely on
        // `BuildOptions::default()` meaning an optimized `-O2` build.
        assert_eq!(BuildOptions::default().opt, OptLevel::O2);
        assert_eq!(OptLevel::default(), OptLevel::O2);
    }

    #[test]
    fn build_writes_executable_to_out() {
        let out = unique_temp("");
        cc_build("int main(void){return 0;}", &out, &BuildOptions::default())
            .expect("should compile");
        assert!(out.exists(), "expected executable at {}", out.display());
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn build_failure_returns_stderr() {
        let out = unique_temp("");
        let err = cc_build("int main(){ broken @@@ }", &out, &BuildOptions::default())
            .expect_err("broken C should fail to compile");
        assert!(!err.is_empty(), "expected non-empty compiler stderr");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn discover_cc_finds_a_compiler() {
        // The CI/dev environment always has at least one of cc/clang/gcc.
        assert!(discover_cc().is_ok());
    }

    #[test]
    fn clang_like_detection() {
        assert!(is_clang_like("clang"));
        assert!(is_clang_like("clang-17"));
        assert!(is_clang_like("/usr/bin/clang"));
        assert!(is_clang_like("/opt/llvm/bin/clang++"));
        assert!(!is_clang_like("gcc"));
        assert!(!is_clang_like("cc"));
        assert!(!is_clang_like("/usr/bin/gcc"));
    }

    #[test]
    fn known_targets_nonempty() {
        let ts = known_targets();
        assert!(!ts.is_empty(), "known_targets must list common triples");
        assert!(ts.contains(&"wasm32-wasi"));
        assert!(ts.contains(&"x86_64-linux-gnu"));
    }

    #[test]
    fn object_only_emits_object_file() {
        // No `main`, no link: a host object compile must still succeed.
        let out = unique_temp(".o");
        let opts = BuildOptions {
            target: None,
            object_only: true,
            opt: OptLevel::O2,
        };
        cc_build("int kard_add(int a, int b){ return a + b; }", &out, &opts)
            .expect("object-only compile should succeed");
        assert!(out.exists(), "expected object file at {}", out.display());
        let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        assert!(len > 0, "object file should be non-empty");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn object_only_cross_compiles_when_clang_available() {
        // A foreign *linked* exe needs a target sysroot, so we only exercise
        // object-only cross emission, and only when clang exists. (SPEC §19.)
        if discover_clang().is_err() {
            eprintln!("skipping cross object_only test: no clang available");
            return;
        }
        let out = unique_temp(".o");
        let opts = BuildOptions {
            target: Some("aarch64-linux-gnu".to_string()),
            object_only: true,
            opt: OptLevel::O2,
        };
        match cc_build("int kard_cross(int a, int b){ return a + b; }", &out, &opts) {
            Ok(()) => {
                assert!(out.exists(), "expected cross object at {}", out.display());
                let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                assert!(len > 0, "cross object file should be non-empty");
            }
            Err(e) => {
                // Some clang builds may lack a particular backend; the contract
                // is only that we surface a clear error and never panic.
                assert!(
                    !e.is_empty(),
                    "a cross failure must yield a non-empty error message"
                );
                eprintln!("cross object_only not supported by this clang: {e}");
            }
        }
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn target_without_clang_errors_clearly() {
        // We can't unset clang in CI, but if no clang is present a targeted
        // build must produce a clear, actionable error rather than silently
        // falling back to a host compiler.
        if discover_clang().is_ok() {
            // clang present: discover_clang succeeds, so this path is covered
            // by the cross-object test instead.
            return;
        }
        let out = unique_temp(".o");
        let opts = BuildOptions {
            target: Some("aarch64-linux-gnu".to_string()),
            object_only: true,
            opt: OptLevel::O2,
        };
        let err = cc_build("int x(void){ return 0; }", &out, &opts)
            .expect_err("targeted build without clang must error");
        assert!(err.contains("clang"), "error should mention clang: {err}");
        let _ = std::fs::remove_file(&out);
    }
}
