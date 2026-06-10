//SPEC: §44 `@writeFile`/`@appendFile` on an unopenable path yield FALSE — no error channel (the §41 convention)
//OUT: 0
//OUT: 0
//OUT: 0
//OUT: 9

pub fn main() void {
    if (@writeFile("/nonexistent_kardc_spec_dir/out.txt", "d")) { print(1); } else { print(0); }
    if (@appendFile("/nonexistent_kardc_spec_dir/out.txt", "d")) { print(1); } else { print(0); }
    if (@writeFile("", "d")) { print(1); } else { print(0); }
    print(9);   // the failures are non-fatal
}
