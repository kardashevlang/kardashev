// error_sets.ks — named error sets (v0.139).
//
// A named error set groups a fixed list of errors:
//
//   const FileErr = error{ NotFound, Denied };
//
// and a function can return an error union *typed over that set* — `FileErr!T`.
// Returning `error.X` where `X` is not a member of the set is a compile error,
// so the set documents (and the compiler checks) exactly what can go wrong.
// At runtime a `Set!T` is just an error union, so `try` / `catch` work as usual.

const ParseErr = error{ Empty, NotADigit, Overflow };

// Parse a small decimal string into an i32 (max 4 digits), or a ParseErr.
fn parse(s: []u8) ParseErr!i32 {
    if (s.len == 0) {
        return error.Empty;
    }
    if (s.len > 4) {
        return error.Overflow;
    }
    var acc: i32 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        var c: u8 = s[i];
        if (c < 48 or c > 57) {        // not '0'..'9'
            return error.NotADigit;
        }
        acc = acc * 10 + @as(i32, c - 48);
    }
    return acc;
}

pub fn main() i32 {
    print(parse("1234") catch 0 - 1);   // 1234
    print(parse("") catch 0 - 1);        // -1 (Empty)
    print(parse("12x") catch 0 - 1);     // -1 (NotADigit)
    print(parse("99999") catch 0 - 1);   // -1 (Overflow, > 4 digits)
    print(parse("7") catch 0 - 1);       // 7

    // `try` propagates the typed error; the propagating fn shares the set.
    var n: i32 = parse("42") catch 0;
    print(n);                            // 42
    return 0;
}

test "named error sets" {
    expect((parse("500") catch 0 - 1) == 500);
    expect((parse("") catch 111) == 111);
    expect((parse("4a") catch 222) == 222);
    expect((parse("8") catch 0) == 8);
}
