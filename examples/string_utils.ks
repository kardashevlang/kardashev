// string_utils.ks — std string helpers over `[]u8` (v0.149).
//
// `@import("std")` brings `str_eq`, `str_starts_with`, `str_index_of` and
// `str_concat` into scope. Strings are `[]u8` slices (v0.127), so these work on
// string literals and on slices you build at runtime.

@import("std");

// Split-ish: does `path` look like a header (`#`-prefixed) or a key=value line?
fn line_kind(line: []u8) i32 {
    if (line.len == 0) {
        return 0;                       // blank
    }
    if (str_starts_with(line, "#")) {
        return 1;                       // comment / header
    }
    if (str_index_of(line, 61) >= 0) {  // contains '='
        return 2;                       // key=value
    }
    return 3;                           // other
}

pub fn main() i32 {
    var a: Allocator = c_allocator();

    if (str_eq("kardashev", "kardashev")) { print(1); } else { print(0); }   // 1
    if (str_eq("a", "b")) { print(1); } else { print(0); }                   // 0
    if (str_starts_with("kardashev", "kard")) { print(1); } else { print(0); } // 1

    print(str_index_of("hello world", 32));   // 5  (the space)
    print(str_index_of("hello", 122));         // -1 ('z' absent)

    var greeting: []u8 = str_concat(a, "Hello, ", "world!");
    print(greeting.len);                       // 13
    print(greeting);                           // Hello, world!
    free(a, greeting);

    print(line_kind("# a comment"));           // 1
    print(line_kind("name=kardashev"));        // 2
    print(line_kind("plain text"));            // 3
    print(line_kind(""));                      // 0
    return 0;
}

test "string utils" {
    var a: Allocator = c_allocator();
    expect(str_eq("foo", "foo"));
    expect(!str_eq("foo", "fox"));
    expect(str_starts_with("foobar", "foo"));
    expect(!str_starts_with("foo", "foobar"));
    expect(str_index_of("abcabc", 99) == 2);   // first 'c'
    expect(str_index_of("abc", 100) == 0 - 1);
    var s: []u8 = str_concat(a, "ab", "cde");
    expect(s.len == 5);
    expect(s[3] == 100);                        // 'd'
    free(a, s);
}
