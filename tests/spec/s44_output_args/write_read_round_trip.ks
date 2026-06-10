//SPEC: §44 `@writeFile` (create/TRUNCATE) then `@readFile` round-trips the exact bytes
//OUT: 1
//OUT: 15
//OUT: kardashev scale
//
// The runner's cwd is unspecified, so the test uses an absolute /tmp path
// (unique to this file). `@writeFile` truncates, so a leftover file from a
// previous run is harmless — and there is no `@deleteFile` to clean up with
// (SPEC §44.3), so the temp file is left behind deliberately.

pub fn main() void {
    var a: Allocator = c_allocator();
    var path: []u8 = "/tmp/kardc_spec_s44_round_trip.tmp";
    if (@writeFile(path, "kardashev scale")) { print(1); } else { print(0); }
    var back: []u8 = @readFile(a, path);
    print(back.len);
    print(back);
    free(a, back);
}
