//SPEC: §34.1 `const Name = error{ A, B };` declares a named set; a `Set!T` function may return any member or a payload
//OUT: 42
//OUT: -1
//OUT: -1
//OUT: 1

const ParseErr = error{ Empty, TooLong };

fn parse(n: i64) ParseErr!i64 {
    if (n == 0) {
        return error.Empty;     // member 1
    }
    if (n > 99) {
        return error.TooLong;   // member 2
    }
    return n * 2;               // payload widens to ParseErr!i64
}

pub fn main() void {
    print(parse(21) catch 0 - 1);    // ok path: 42
    print(parse(0) catch 0 - 1);     // Empty -> default -1
    print(parse(100) catch 0 - 1);   // TooLong -> default -1
    // The two members are distinct error values (distinct codes).
    var c1: i64 = parse(0) catch |e| @as(i64, e);
    var c2: i64 = parse(100) catch |e| @as(i64, e);
    if (c1 == c2) {
        print(0);
    } else {
        print(1);
    }
}
