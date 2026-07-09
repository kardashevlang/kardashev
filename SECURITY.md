# Security Policy

This is the coordinated security-response process for the kardashev toolchain
— the `kard` binary (compiler, build system, test runner, formatter), the
embedded standard library (`crates/kardc/src/std.ks`), and the self-hosted
compiler sources under `selfhost/`.

## Supported versions

Until 1.0, only the **latest released minor** receives security fixes. After
1.0, the stability policy (editions + SemVer; see
[ROADMAP-RUST-ZIG.md](ROADMAP-RUST-ZIG.md)) will define the support window per
release line.

## Reporting a vulnerability

Report privately — **do not** open a public issue for an unfixed vulnerability.

- Use GitHub's private **"Report a vulnerability"** advisory form on this
  repository (Security tab), or email the maintainer (the GitHub account on
  the repository) with `SECURITY` in the subject.
- Include: the affected component, the toolchain version (`kard version`), a
  minimized reproduction, the observed impact, and any suggested fix.

## Response process & SLA

| Stage | Target |
|-------|--------|
| Acknowledge receipt | within 3 business days |
| Triage + severity assessment | within 7 business days |
| Fix + coordinated release | within 90 days (sooner for actively-exploited / critical) |
| Public disclosure | after a fix ships, or at the embargo deadline |

Embargo: details are held until a fixed release is available (max 90 days),
then disclosed with credit (unless the reporter requests anonymity).

## Threat model

What counts as a security bug here:

1. **Compiler robustness on adversarial source.** `kard` must never crash
   (SIGSEGV/SIGABRT/panic) on any *source* input, valid or hostile — it must
   report a clean diagnostic and exit non-zero. The diagnostic paths are
   pinned by the `tests/spec/` conformance corpus (its `//ERR:` cases); a
   reproducible crash on crafted input is a reportable bug. (A
   coverage-guided fuzzer for the front end is a roadmap item, not yet a CI
   gate.)
2. **Miscompilation of a documented safety check.** kardashev is a systems
   language with raw pointers and manual memory management — programs *can*
   be written unsafely, and that is by design, documented in
   [SPEC.md](SPEC.md). But where the language **does** promise a runtime
   check — bounds-checked array/slice indexing and slicing (panic + exit 101
   on violation), checked `.?` unwraps, exhaustive `switch` — emitted C that
   silently skips such a check is a security bug, not just a correctness bug.
3. **Toolchain behaviour.** `kard` shells out to exactly one external
   program: the system C compiler (`$CC`, else the first of `cc`, `clang`,
   `gcc` found). Any other process execution, network access, or file access
   outside the build's inputs/outputs (and the OS temp directory) would be a
   bug. Note that `kard run`, `test` and `bench` **execute the compiled
   program** — running untrusted source means running untrusted code, exactly
   as with any native compiler.

## Supply chain

The toolchain is plain Rust with **zero external crates** — it builds against
the Rust standard library alone, so there is no third-party dependency tree
to audit or advisory-track. Generated programs depend only on the target's
libc, via the emitted portable C11. A package registry (and with it a
`kard audit`) is a far-future roadmap item; this policy will extend to the
ecosystem when one exists.
