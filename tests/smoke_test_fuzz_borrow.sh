#!/usr/bin/env bash
# v66 — borrow-checker DIFFERENTIAL FUZZER. A seeded generator emits 100+ random
# programs from hand-classified TEMPLATES exercising mutable refs, reborrows,
# ref returns, field/tuple access through refs, closure captures, and
# match-through-&T. Each template carries a SOUND/UNSOUND oracle: every SOUND
# program MUST compile, every UNSOUND one MUST be rejected — zero false
# pos/neg. (Template-based, not free-form, precisely so the oracle can't bless a
# false-negative; a sample is hand-verified below the generator.) Seeded for
# reproducibility.
set -uo pipefail
KARDC=""
for c in "${TEST_SRCDIR:-}/_main/compiler/kardc" "${RUNFILES_DIR:-}/_main/compiler/kardc" "./compiler/kardc" "./build.local/kardc"; do
  [[ -n "$c" && -x "$c" ]] && { KARDC="$c"; break; }; done
[[ -z "$KARDC" ]] && { echo "FAIL: kardc not found"; exit 1; }
echo "Using kardc at: $KARDC"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
SEED="${FUZZ_SEED:-66066}"; RANDOM=$SEED
N="${FUZZ_N:-120}"

# Each template prints a program given two random ints ($1=a, $2=b). Templates
# 1..8 are SOUND; 9..14 are UNSOUND. (Tag in `oracle`.)
gen() { local t=$1 a=$2 b=$3
  case $t in
  1) printf 'fn main() -> i64 { let v = %d; let r = &v; *r + %d - %d }\n' "$a" "$b" "$b" ;;            # shared borrow read
  2) printf 'fn main() -> i64 { let mut v = %d; { let r = &mut v; *r = %d; } v }\n' "$a" "$b" ;;        # sequenced &mut then read
  3) printf 'fn pick(x: &i64) -> &i64 { x }\nfn main() -> i64 { let v = %d; *pick(&v) + %d - %d }\n' "$a" "$b" "$b" ;; # ref return rooted in ref param
  4) printf 'struct P { a: i64, b: i64 }\nfn main() -> i64 { let p = P { a: %d, b: %d }; let r = &p; r.a }\n' "$a" "$b" ;; # field thru ref
  5) printf 'fn main() -> i64 { let t = (%d, %d); let r = &t; r.0 }\n' "$a" "$b" ;;                      # tuple thru ref
  6) printf 'fn main() -> i64 { let o = Some(%d); match &o { Some(x) => *x, None => %d } }\n' "$a" "$b" ;; # match thru &T
  7) printf 'fn main() -> i64 ! { alloc } { let mut v = vec_new(); vec_push(&mut v, %d); vec_push(&mut v, vec_len(&v)); vec_get(&v, 0) }\n' "$a" ;; # two-phase borrow
  8) printf 'fn main() -> i64 { let n = %d; let f = |x| x + n; f(%d) }\n' "$a" "$b" ;;                   # closure capture by value (Fn)
  9) printf 'struct W { s: String, id: i64 }\nfn main() -> i64 ! { alloc } { let a = W { s: int_to_string(%d), id: %d }; let b = a; a.id }\n' "$a" "$b" ;; # use after move
  10) printf 'fn main() -> i64 { let mut v = %d; let r1 = &mut v; let r2 = &mut v; *r1 + *r2 + %d }\n' "$a" "$b" ;; # two &mut at once
  11) printf 'fn main() -> i64 { let mut v = %d; let r = &v; let m = &mut v; *m = %d; *r }\n' "$a" "$b" ;; # &mut while & live
  12) printf 'fn dangle() -> &i64 { let x = %d; &x }\nfn main() -> i64 { *dangle() + %d - %d }\n' "$a" "$b" "$b" ;; # return ref to local
  13) printf 'fn main() -> i64 { let x = %d; x = %d; x }\n' "$a" "$b" ;;                                 # assign to immutable
  14) printf 'struct H { r: i64 }\nfn leak() -> H { let x = %d; H { r: x } }\nfn main() -> i64 { %d }\n' "$a" "$b" ;; # (sound control: H holds an i64 copy, not a ref) -> SOUND
  esac
}
oracle() { case $1 in 9|10|11|12|13) echo UNSOUND;; *) echo SOUND;; esac; }  # 14 is a SOUND control

pass=0; sound_ok=0; unsound_ok=0
for ((i=0; i<N; i++)); do
  t=$(( RANDOM % 14 + 1 )); a=$(( RANDOM % 1000 )); b=$(( RANDOM % 1000 ))
  gen "$t" "$a" "$b" > "$TMP/p.kd"
  want=$(oracle "$t")
  if "$KARDC" --no-cache -o "$TMP/p" "$TMP/p.kd" >/dev/null 2>&1; then got=SOUND; else got=UNSOUND; fi
  if [[ "$got" == "$want" ]]; then
    pass=$((pass+1)); [[ "$want" == SOUND ]] && sound_ok=$((sound_ok+1)) || unsound_ok=$((unsound_ok+1))
  else
    echo "FAIL [template $t, seed run $i]: oracle=$want got=$got"
    cat "$TMP/p.kd"; "$KARDC" "$TMP/p.kd" 2>&1 | head -3; exit 1
  fi
done
echo "PASS: $pass/$N programs matched the borrow oracle (sound-accepted=$sound_ok, unsound-rejected=$unsound_ok)"
(( sound_ok > 0 && unsound_ok > 0 )) || { echo "FAIL: degenerate run (need both sound + unsound exercised)"; exit 1; }

# Hand-verified sanity: one canonical instance of each UNSOUND template MUST be
# rejected (guards against the oracle silently blessing a false-negative).
for t in 9 10 11 12 13; do
  gen "$t" 5 7 > "$TMP/u.kd"
  if "$KARDC" --no-cache -o "$TMP/u" "$TMP/u.kd" >/dev/null 2>&1; then
    echo "FAIL: UNSOUND template $t COMPILED — a real soundness hole!"; cat "$TMP/u.kd"; exit 1
  fi
done
echo "PASS: every UNSOUND template's canonical instance is rejected (no false-negative)"
echo "ALL BORROW-FUZZER TESTS PASSED (seed $SEED)"
