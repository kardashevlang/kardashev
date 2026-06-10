//SPEC: §44 an out-of-range `@arg` index — too large or negative — yields an EMPTY slice, not a trap
//OUT: 0
//OUT: 0
//OUT: 9

pub fn main() void {
    var a: Allocator = c_allocator();
    print(@arg(a, 9999).len);    // 0 — ≥ @argc()
    print(@arg(a, 0 - 1).len);   // 0 — negative
    print(9);                    // and the program carries on
}
