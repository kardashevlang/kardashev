//SPEC: §12.1 `expr catch default` yields the payload of an ok `!T`, else `default`; the result is a plain `T`
//OUT: 407
//OUT: 9

fn parse_digit(c: u8) !i64 {
    if (c < 48) {
        return error.NotADigit;
    }
    if (c > 57) {
        return error.NotADigit;
    }
    return @as(i64, c) - 48;
}

pub fn main() void {
    // Fold a string into a number, substituting 0 for the bad byte via
    // `catch` — the result is a plain i64 the accumulator can use.
    var s: []u8 = "4x7";
    var total: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i = i + 1) {
        total = total * 10 + (parse_digit(s[i]) catch 0);
    }
    print(total);                          // 4, 0 ('x'), 7 -> 407
    print(parse_digit(57) catch 0 - 1);    // '9' parses: payload 9, not -1
}
