// switch_ranges.ks — switch range labels + multi-label arms (v0.146).
//
//   switch (n) {
//       1, 2, 3 => { … }   // several labels share an arm (since v0.116)
//       10..20  => { … }   // an inclusive integer range [10, 20] (v0.146)
//       else    => { … }
//   }
//
// Ranges and labels combine in one arm, lowering to GNU C case-ranges.

fn letter_grade(score: i32) []u8 {
    switch (score) {
        90..100 => { return "A"; },
        80..89  => { return "B"; },
        70..79  => { return "C"; },
        60..69  => { return "D"; },
        else    => { return "F"; },
    }
}

// Classify a byte: digit / lower / upper / other, mixing single values + ranges.
fn char_class(c: i32) i32 {
    switch (c) {
        48..57  => { return 1; },   // '0'..'9'
        65..90  => { return 2; },   // 'A'..'Z'
        97..122 => { return 3; },   // 'a'..'z'
        32, 9, 10 => { return 4; }, // space, tab, newline (multi-label)
        else    => { return 0; },
    }
}

pub fn main() i32 {
    print(letter_grade(95));   // A
    print(letter_grade(83));   // B
    print(letter_grade(71));   // C
    print(letter_grade(40));   // F

    print(char_class(53));     // 1  ('5')
    print(char_class(81));     // 2  ('Q')
    print(char_class(104));    // 3  ('h')
    print(char_class(32));     // 4  (space)
    print(char_class(36));     // 0  ('$')
    return 0;
}

test "switch ranges" {
    expect(char_class(48) == 1);    // '0' lower bound
    expect(char_class(57) == 1);    // '9' upper bound
    expect(char_class(90) == 2);    // 'Z' upper bound
    expect(char_class(10) == 4);    // newline (multi-label)
    var a: []u8 = letter_grade(100);
    expect(a[0] == 65);             // 'A'
}
