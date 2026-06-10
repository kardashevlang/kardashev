// write_args.ks — file output + program arguments (v0.158, SPEC §44).
//
//   @writeFile(path, data)   -> bool   write the whole file (create/truncate)
//   @appendFile(path, data)  -> bool   append (create if missing)
//   @argc()                  -> i64    argument count, INCLUDING argv[0]
//   @arg(a, i)               -> []u8   i-th argument, freshly allocated
//                                      (empty slice when out of range)
//
// Run:  kard run examples/write_args.ks -- hello world
//
// The writes report success as a bool (no error channel — the §41
// convention); an unopenable path simply yields false. The file round-trip
// below is idempotent: @writeFile truncates, so re-running starts fresh
// (there is no @deleteFile, so the temp file is left in /tmp).

pub fn main() i32 {
    var a: Allocator = c_allocator();

    // Echo the command line. @argc() counts argv[0], so user arguments are
    // indices 1 .. @argc()-1.
    print(@argc());
    var i: i64 = 1;
    while (i < @argc()) : (i = i + 1) {
        var s: []u8 = @arg(a, i);
        print(s);
        free(a, s);
    }

    // Round-trip a file: truncate-write, append, read back.
    var path: []u8 = "/tmp/kardashev_write_args_example.tmp";
    if (@writeFile(path, "kardashev ")) { print(1); } else { print(0); }
    if (@appendFile(path, "scale")) { print(1); } else { print(0); }
    var back: []u8 = @readFile(a, path);
    print(back.len);    // 15
    print(back);        // kardashev scale
    free(a, back);
    return 0;
}
