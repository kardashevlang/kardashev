//SPEC: §44 `@writeFile` with empty data succeeds and truncates an existing file to zero bytes
//OUT: 1
//OUT: 3
//OUT: 1
//OUT: 0

pub fn main() void {
    var a: Allocator = c_allocator();
    var p: []u8 = "/tmp/kardc_spec_s44_truncate_empty.tmp";
    if (@writeFile(p, "abc")) { print(1); } else { print(0); }
    var d1: []u8 = @readFile(a, p);
    print(d1.len);   // 3
    if (@writeFile(p, "")) { print(1); } else { print(0); }
    var d2: []u8 = @readFile(a, p);
    print(d2.len);   // truncated to 0 (the file exists, so the read succeeds)
    free(a, d1);
    free(a, d2);
}
