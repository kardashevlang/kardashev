# Security Policy

This is the coordinated security-response process for the kardashev toolchain
(`kardc`, `kard`, `kard-lsp`, the standard library, and the package registry
once hosted). It is a 1.0 readiness requirement (Roadmap v46) and is referenced
by the `[K-*]` clauses of [docs/SPEC.md](docs/SPEC.md).

## Supported versions

Until 1.0, only the **latest released minor** receives security fixes. After
1.0, the stability policy (editions + SemVer; Roadmap v46) defines the support
window per release line.

## Reporting a vulnerability

Report privately — **do not** open a public issue for an unfixed vulnerability.

- Email the maintainer (the GitHub account on the repository) with `SECURITY` in
  the subject, or use GitHub's private "Report a vulnerability" advisory form.
- Include: affected component + version (`kardc --version`), a minimized repro,
  impact, and any suggested fix.

## Response process & SLA

| Stage | Target |
|-------|--------|
| Acknowledge receipt | within 3 business days |
| Triage + severity (CVSS) | within 7 business days |
| Fix + coordinated release | within 90 days (sooner for actively-exploited / critical) |
| Public disclosure | after a fix ships, or at the embargo deadline |

Embargo: details are held until a fixed release is available (max 90 days), then
disclosed with credit (unless the reporter requests anonymity).

## Threat model & hardening

Two distinct safety surfaces, each with a CI gate:

1. **The compiler as a service (DoS / crash-resistance).** `kardc` must never
   crash (SIGSEGV/SIGABRT) on adversarial *source* input — it must report a
   clean diagnostic. Enforced by `tests/smoke_test_compiler_fuzz.sh` (curated
   adversarial corpus + random token soup; a crash fails CI). The
   parser/typechecker/borrow-checker are the DoS surface here. A full
   `fuzz_compiler` (≥100k inputs/run, ASan/UBSan-clean) is the remaining v46
   gate.
2. **Generated-program memory safety.** Code `kardc` emits must be memory-safe;
   the safe subset is sanitizer- and differential-fuzzer-gated (v37/v47), and
   the `unsafe` surface is documented by the `[K-own]`/`[K-borrow]`/`[K-panic]`/
   `[K-abi]` clauses + the aliasing model.

## Supply chain (registry — roadmapped)

When the package registry (Mega-arc B) ships, `kard audit` consumes a published
advisory database (non-zero exit on a vulnerable dependency), `kard license`
flags incompatible licenses, and the registry verifies reproducible builds +
SLSA provenance before accepting a publish. The advisory schema + embargo
process for *ecosystem* CVEs extend this policy. (Not yet hosted — see
ROADMAP-1.0-AND-BEYOND.md, Mega-arc B.)
