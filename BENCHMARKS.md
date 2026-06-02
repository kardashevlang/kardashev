# Benchmarks

Reproducible runtime numbers for kardashev's AOT output vs an equivalent C
reference — turning "performance unmeasured" (the honest gap noted in
[ROADMAP.md](ROADMAP.md)) into actual data.

**Method.** Each workload is written identically in kardashev and C. The
kardashev program is AOT-compiled with `kardc -O2` (LLVM, linked via `clang`);
the C reference with `clang -O2`. Both are run best-of-3 (lowest wall-clock), and
their outputs are checked to be identical. Reproduce with:

```sh
make -f Makefile.local kardc      # or: bazel build //compiler:kardc
bench/run.sh
```

## Results

Measured on this dev box (x86-64, LLVM 21, `clang -O2` for both back-ends).
Absolute times vary by machine; the **ratio to C** is the portable figure.

| Workload  | What it stresses                    | kardashev | C (clang -O2) | ratio |
|-----------|-------------------------------------|-----------|---------------|-------|
| `fib`     | recursion + function-call overhead  | ~0.23 s   | ~0.23 s       | **1.00×** |
| `collatz` | branches + signed `/` `%`           | ~0.37 s   | ~0.37 s       | **~1.01×** |
| `primes`  | nested loops + `%` (app-scale)      | ~0.015 s  | ~0.014 s      | **~1.07×** |
| `loop`    | a tight integer-arithmetic loop     | ~0.11 s   | ~0.05 s       | **~2.2×** |
| `matmul`  | 64×64 int matmul (correctness only) | ~2.5 s    | ~0.001 s      | *n/a*     |

`fib(40)`, the Collatz step-count over `1..3,000,000`, prime-counting (trial
division) under 200,000, a 200 M-iteration arithmetic loop, and a 64×64 integer
matrix multiply. **All five produce the same result as the C reference**
(correctness-gated in CI by `smoke_test_bench.sh`).

**v44 application-scale update.** `primes` is the headline new figure: on a real,
non-trivial integer workload (not a micro-loop) kardashev runs at **~1.07× C —
inside the 1.1× parity target**, and `collatz` is at parity (~1.0×). The `matmul`
ratio is *not* a fair runtime comparison and is marked correctness-only: with a
fully deterministic result and `static` arrays, `clang -O2` constant-folds the
entire computation to a compile-time constant (~0.001 s), so it measures clang's
folding, not codegen quality — kardashev does the actual work. It is kept as a
correctness benchmark (array indexing + nested loops, output == C).

## Reading these honestly

- kardashev's AOT codegen is **C-competitive** on call-heavy and branch-heavy
  code (`fib`, `collatz` ≈ 1.0×) — unsurprising, since it shares LLVM's `-O2`
  pipeline with `clang`.
- The **~2.2× gap on the tight `loop`** is real and the most interesting figure:
  the simplest counted integer loop is where kardashev currently trails C the
  most (the front-end's alloca-heavy lowering of `let mut` counters + the signed
  division leave the loop less optimized than clang's). This is a concrete
  codegen-optimization target, not a fundamental limit.
- These are **micro-benchmarks**, not a representative application workload, and
  they exercise only the scalar/integer path (no allocation, GC-free by design,
  no I/O). They establish that the compiler emits real, reasonable native code —
  they do **not** claim kardashev is "as fast as C" in general.
