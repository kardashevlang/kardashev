// QUARANTINED (wave B, §30): the compiler is more permissive than SPEC §30.2/§15.1.
//
// SPEC §30.2 requires a pointer-receiver call's receiver to be "an addressable
// lvalue (a `var`, field, or index; else an error, as for `&`)", and §15.1's
// `&place` likewise lists "a `var`, a field chain, an index, or a deref" —
// a `const` local is in NEITHER list, so `p.inc()` / `&p` on a const binding
// reads as an error per the SPEC. The compiler instead accepts BOTH and the
// pointer-receiver method mutates the const binding in place:
//
//   const p = P{ .x = 5 };
//   p.inc();          // compiles; prints 6 — a `const` was mutated
//
// (sema's `is_addressable_place` checks only the expression SHAPE — any Ident
// passes, const or not.) Either the compiler should reject const receivers /
// `&const`, or the SPEC's lvalue lists should say "binding" instead of "var".
// Not pinned in the corpus either way until one of the two is amended.

const P = struct {
    x: i64,

    fn inc(self: *P) void {
        self.x += 1;
    }
};

pub fn main() void {
    const p = P{ .x = 5 };
    p.inc();           // accepted today; SPEC reads as an error
    print(p.x);        // prints 6 today (a mutated const)

    const n: i64 = 5;
    var q: *i64 = &n;  // `&` on a const local — same gray zone (§15.1)
    q.* = 7;
    print(n);          // prints 7 today
}
