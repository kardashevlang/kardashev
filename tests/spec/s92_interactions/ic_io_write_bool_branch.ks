//SPEC: §44.1 `@writeFile` yields a bool usable directly in conditions and `and` chains
//OUT: 1
//OUT: 0

pub fn main() void {
    var path: []u8 = "/tmp/kardc_spec_s92_write_bool.tmp";
    if (@writeFile(path, "x") and @writeFile(path, "y")) { print(1); } else { print(0); }
    // A path through a missing directory cannot be opened: false.
    if (@writeFile("/tmp/kardc_spec_s92_no_such_dir/x.tmp", "x")) { print(1); } else { print(0); }
}
