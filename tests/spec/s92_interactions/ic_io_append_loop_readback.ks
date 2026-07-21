//SPEC: §44 x §29 `@appendFile` in a loop accumulates bytes; `@readFile` sees the whole concatenation
//OUT: 1
//OUT: 9
//OUT: abcabcabc
//
// `@writeFile` truncates first so a leftover file from a previous run is
// harmless (there is no `@deleteFile` to clean up with, SPEC §44.3).

pub fn main() void {
    var a: Allocator = c_allocator();
    var path: []u8 = "/tmp/kardc_spec_s92_append_loop.tmp";
    if (@writeFile(path, "")) { print(1); } else { print(0); }
    var i: i64 = 0;
    while (i < 3) : (i += 1) {
        if (@appendFile(path, "abc")) {} else { print(99); }
    }
    var back: []u8 = @readFile(a, path);
    print(back.len);   // 3 x 3 bytes
    print(back);
    free(a, back);
}
