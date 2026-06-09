// io.ks — minimal input: `@readFile` and `@readLine` (v0.148).
//
//   @readFile(a, path)  -> []u8   read a whole file (empty []u8 on error)
//   @readLine(a)        -> []u8   read one line from stdin (newline stripped)
//
// Both allocate their result on the given Allocator (free it with free(a, s)).
// There is no `![]u8`, so an open/read failure is reported as an empty slice.
//
// Run interactively:   echo "hello" | kard run examples/io.ks
// (with no stdin, @readLine yields a zero-length slice and the program still
// runs — handy for non-interactive checks.)

@import("std");

// Count the bytes in `s` equal to `target`.
fn count(s: []u8, target: u8) i32 {
    var n: i32 = 0;
    for (s) |b| {
        if (b == target) {
            n += 1;
        }
    }
    return n;
}

pub fn main() i32 {
    var a: Allocator = c_allocator();

    // Read a line from stdin and report its length + a vowel count.
    var line: []u8 = @readLine(a);
    print(line.len);                 // number of bytes on the first stdin line
    var vowels: i32 = count(line, 97) + count(line, 101) + count(line, 105)
        + count(line, 111) + count(line, 117);   // a e i o u
    print(vowels);
    free(a, line);

    // A missing file yields an empty slice (no crash).
    var missing: []u8 = @readFile(a, "this-file-does-not-exist.kd");
    print(missing.len);              // 0
    free(a, missing);

    print(imax(3, 8));               // 8 (std is in scope too)
    return 0;
}
