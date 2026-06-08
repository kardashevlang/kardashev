// strings.ks — string literals as `[]u8` values (v0.127).
//
// A string literal is a `[]u8` slice over static bytes, so the slice operations
// apply: `.len`, indexing `s[i]` (a `u8` byte), and sub-slicing `s[lo..hi]`.
// `print` accepts a string (writes the bytes + a newline) as well as integers.

fn shout(msg: []u8) void {
    print(msg);
}

// Count the bytes equal to `target` in a string.
fn count_byte(s: []u8, target: u8) i32 {
    var n: i32 = 0;
    var i: usize = 0;
    while (i < s.len) : (i = i + 1) {
        if (s[i] == target) {
            n = n + 1;
        }
    }
    return n;
}

pub fn main() i32 {
    shout("Hello, kardashev!");

    var word: []u8 = "banana";
    print(word.len);                  // 6
    print(count_byte(word, 97));      // 3  ('a' == 97)
    print(word[0]);                   // 98 ('b')

    var tail: []u8 = word[2..6];      // "nana"
    print(tail.len);                  // 4
    print(tail);                      // nana
    return 0;
}

test "string ops" {
    var s: []u8 = "kardashev";
    expect(s.len == 9);
    expect(s[0] == 107);              // 'k'
    expect(count_byte(s, 97) == 2);   // two 'a's
}
