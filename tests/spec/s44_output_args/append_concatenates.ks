//SPEC: §44 `@appendFile` appends to the truncate-written file — read-back sees the concatenation
//OUT: 1
//OUT: 1
//OUT: abcd
//
// `@writeFile` first (truncate) makes the test deterministic across runs even
// though the /tmp file persists (no `@deleteFile`, SPEC §44.3).

pub fn main() void {
    var a: Allocator = c_allocator();
    var path: []u8 = "/tmp/kardc_spec_s44_append.tmp";
    if (@writeFile(path, "ab")) { print(1); } else { print(0); }
    if (@appendFile(path, "cd")) { print(1); } else { print(0); }
    var back: []u8 = @readFile(a, path);
    print(back);
    free(a, back);
}
