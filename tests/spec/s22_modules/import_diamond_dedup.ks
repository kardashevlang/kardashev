//SPEC: §22.1 a file reachable twice (diamond) is included ONCE — its items do not collide with themselves
//OUT: 31

// _d_left.ks and _d_right.ks both import _d_base.ks (const D_TEN + fn d_base).
// Double inclusion would make every base item an E0293 duplicate, so this
// program compiling at all pins the visited-path dedup; the value checks the
// arms really share the one base: (10 + 1) + (10 * 2) = 31.
@import("_d_left.ks");
@import("_d_right.ks");

pub fn main() void {
    print(d_left() + d_right());
}
