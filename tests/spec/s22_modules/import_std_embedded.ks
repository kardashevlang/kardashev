//SPEC: §22.1 `@import("std")` resolves to the standard library EMBEDDED in the compiler — no std file on disk
//OUT: 9
//OUT: 5
//OUT: 7

// No file named `std`/`std.ks` exists in this directory; the import still
// resolves (to the bundled crates/kardc/src/std.ks) and its symbols work.
@import("std");

pub fn main() void {
    print(imax(3, 9));     // i32 max
    print(iabs(0 - 5));    // i32 abs
    print(imin64(7, 8));   // i64 min
}
