//SPEC: §22.1 the embedded std dedups like any module — imported by BOTH the root and a helper file without E0293
//OUT: 2
//OUT: 42

@import("std");
@import("_uses_std.ks");

pub fn main() void {
    print(helper_min());        // fixture's imin(4, 2) through its own std import
    print(imax(1, 40) + 2);     // the root's own std import
}
