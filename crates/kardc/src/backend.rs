//! Native backend driver: C source → object/executable via the system C
//! compiler, plus execution.
//!
//! This module owns *only* the cc invocation and process execution. The
//! lex→parse→sema→emit pipeline lives in [`crate::compile_to_c`].

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Monotonic counter giving each temp path a process-unique suffix even when
/// several builds run concurrently (e.g. parallel `cargo test` threads, which
/// share a PID).
static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Resolve the C compiler to use.
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
        let ok = Command::new(cand)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Ok(cand.to_string());
        }
    }
    Err("no C compiler found (tried $CC, then `cc`, `clang`, `gcc`)".to_string())
}

/// Build a process-unique path under the system temp directory. `suffix` is
/// appended verbatim (e.g. `".c"` for sources, `""` for executables).
fn unique_temp(suffix: &str) -> PathBuf {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("kardc_{}_{}{}", std::process::id(), n, suffix));
    path
}

/// Invoke `<cc> -O2 -std=c11 -o <out> <tmp_c>`, returning the compiler's
/// stderr on a non-zero exit.
fn invoke_cc(cc: &str, tmp_c: &Path, out: &Path) -> Result<(), String> {
    let output = Command::new(cc)
        .arg("-O2")
        .arg("-std=c11")
        .arg("-o")
        .arg(out)
        .arg(tmp_c)
        .output()
        .map_err(|e| format!("failed to invoke C compiler `{cc}`: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// Compile C source text to a native executable at `out` using the system C
/// compiler. Returns the compiler's stderr on failure.
pub fn cc_build(c_src: &str, out: &Path) -> Result<(), String> {
    let cc = discover_cc()?;
    let tmp_c = unique_temp(".c");
    std::fs::write(&tmp_c, c_src)
        .map_err(|e| format!("failed to write temp C file {}: {e}", tmp_c.display()))?;
    let result = invoke_cc(&cc, &tmp_c, out);
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

/// Compile C source text to a temporary executable, run it, and return the
/// child process exit code. Returns an error string on a compile failure.
pub fn cc_build_and_run(c_src: &str, args: &[String]) -> Result<i32, String> {
    let exe = unique_temp("");
    cc_build(c_src, &exe)?;
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
        let code = cc_build_and_run("int main(){return 7;}", &[]).expect("should compile and run");
        assert_eq!(code, 7);
    }

    #[test]
    fn run_program_that_prints() {
        let src = "#include <stdio.h>\nint main(void){ printf(\"hello from kardc\\n\"); return 0; }";
        let code = cc_build_and_run(src, &[]).expect("should compile and run");
        assert_eq!(code, 0);
    }

    #[test]
    fn broken_program_errs() {
        let result = cc_build_and_run("int main(){ this is not valid C @@@ }", &[]);
        assert!(result.is_err(), "broken C should yield Err, got {result:?}");
    }

    #[test]
    fn build_writes_executable_to_out() {
        let out = unique_temp("");
        cc_build("int main(void){return 0;}", &out).expect("should compile");
        assert!(out.exists(), "expected executable at {}", out.display());
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn build_failure_returns_stderr() {
        let out = unique_temp("");
        let err = cc_build("int main(){ broken @@@ }", &out)
            .expect_err("broken C should fail to compile");
        assert!(!err.is_empty(), "expected non-empty compiler stderr");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn discover_cc_finds_a_compiler() {
        // The CI/dev environment always has at least one of cc/clang/gcc.
        assert!(discover_cc().is_ok());
    }
}
