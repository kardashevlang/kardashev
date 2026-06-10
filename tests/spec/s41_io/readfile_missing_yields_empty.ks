//SPEC: §41 `@readFile` on an unopenable path yields an EMPTY slice — no error channel (no ![]u8, §11/§12)
//OUT: 0
//OUT: 9

pub fn main() void {
    var a: Allocator = c_allocator();
    var missing: []u8 = @readFile(a, "/nonexistent_kardc_spec_dir/no_such_file.txt");
    print(missing.len);   // 0 — the open failed, the program carries on
    // The failure is non-fatal and leaves the allocator usable.
    var buf: []u8 = alloc(a, u8, 9);
    print(buf.len);
    free(a, buf);
}
