//SPEC: §30.2 a pointer-receiver call's receiver must not be rooted in a `const` binding — the auto-ref `&obj` would mutate a `const` (E0233)
//ERR: E0233

// Was quarantined (wave B, v0.156): the compiler accepted `p.inc()` on a
// `const p` and mutated it in place (`is_addressable_place` checked only the
// expression SHAPE). SPEC §30.2/§15.1 list a `var`, field, or index — a
// `const` local is in neither list. Decision: reject (nothing in the corpus,
// std or examples depended on the permissive behaviour); sema now classifies
// bindings var/param/const and E0233 names the offending binding.

const P = struct {
    x: i64,

    fn inc(self: *P) void {
        self.x += 1;
    }
};

pub fn main() void {
    const p = P{ .x = 5 };
    p.inc(); // error: pointer-receiver call on a `const` binding
    print(p.x);
}
