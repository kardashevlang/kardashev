// tests/std/text.ks — std text module: parse_i64 / fmt_i64 / fmt_u64_hex,
// str_ends_with / str_last_index_of / str_count, StrBuilder.

@import("std");

test "parse_i64 basics" {
    expect((parse_i64("0") orelse 7) == 0);
    expect((parse_i64("7") orelse 0) == 7);
    expect((parse_i64("123") orelse 0) == 123);
    expect((parse_i64("-1") orelse 0) == 0 - 1);
    expect((parse_i64("-987654321") orelse 0) == 0 - 987654321);
    expect((parse_i64("0042") orelse 0) == 42);     // leading zeros OK
    expect((parse_i64("-0") orelse 1) == 0);
}

test "parse_i64 rejects garbage" {
    expect((parse_i64("") orelse 11) == 11);        // empty
    expect((parse_i64("-") orelse 22) == 22);       // sign alone
    expect((parse_i64("+5") orelse 33) == 33);      // no '+' support
    expect((parse_i64("12a3") orelse 44) == 44);    // embedded letter
    expect((parse_i64(" 1") orelse 55) == 55);      // leading space
    expect((parse_i64("1 ") orelse 66) == 66);      // trailing space
    expect((parse_i64("--5") orelse 77) == 77);     // double sign
    expect((parse_i64("1.5") orelse 88) == 88);     // not an int
}

test "parse_i64 extremes" {
    var mx: i64 = 9223372036854775807;
    var mn: i64 = (0 - 9223372036854775807) - 1;
    expect((parse_i64("9223372036854775807") orelse 0) == mx);
    expect((parse_i64("-9223372036854775808") orelse 0) == mn);
    expect((parse_i64("9223372036854775808") orelse 99) == 99);    // max+1 → null
    expect((parse_i64("-9223372036854775809") orelse 88) == 88);   // min-1 → null
    expect((parse_i64("99999999999999999999") orelse 77) == 77);   // way over
    expect((parse_i64("-99999999999999999999") orelse 66) == 66);
}

test "fmt_i64" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_i64(a, 0);
    expect(str_eq(s, "0"));
    free(a, s);
    s = fmt_i64(a, 7);
    expect(str_eq(s, "7"));
    free(a, s);
    s = fmt_i64(a, 42);
    expect(str_eq(s, "42"));
    free(a, s);
    s = fmt_i64(a, 0 - 1);
    expect(str_eq(s, "-1"));
    free(a, s);
    s = fmt_i64(a, 0 - 1000);
    expect(str_eq(s, "-1000"));
    free(a, s);
    s = fmt_i64(a, 9223372036854775807);
    expect(str_eq(s, "9223372036854775807"));
    free(a, s);
    s = fmt_i64(a, (0 - 9223372036854775807) - 1);
    expect(str_eq(s, "-9223372036854775808"));
    free(a, s);
}

test "fmt/parse round-trip property" {
    var a: Allocator = c_allocator();
    var vals: [8]i64 = [8]i64{
        0,
        1,
        0 - 1,
        10,
        0 - 10,
        123456789,
        9223372036854775807,
        (0 - 9223372036854775807) - 1,
    };
    for (vals) |v| {
        var s: []u8 = fmt_i64(a, v);
        expect((parse_i64(s) orelse 424242) == v);   // sentinel not in vals
        free(a, s);
    }
}

test "fmt_u64_hex table" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_u64_hex(a, 0);
    expect(str_eq(s, "0"));
    free(a, s);
    s = fmt_u64_hex(a, 1);
    expect(str_eq(s, "1"));
    free(a, s);
    s = fmt_u64_hex(a, 10);
    expect(str_eq(s, "a"));
    free(a, s);
    s = fmt_u64_hex(a, 15);
    expect(str_eq(s, "f"));
    free(a, s);
    s = fmt_u64_hex(a, 16);
    expect(str_eq(s, "10"));
    free(a, s);
    s = fmt_u64_hex(a, 255);
    expect(str_eq(s, "ff"));
    free(a, s);
    s = fmt_u64_hex(a, 256);
    expect(str_eq(s, "100"));
    free(a, s);
    s = fmt_u64_hex(a, 4096);
    expect(str_eq(s, "1000"));
    free(a, s);
    s = fmt_u64_hex(a, 305441741);             // 0x1234abcd
    expect(str_eq(s, "1234abcd"));
    free(a, s);
    s = fmt_u64_hex(a, 9223372036854775807);   // i64 max
    expect(str_eq(s, "7fffffffffffffff"));
    free(a, s);
    var m: u64 = 9223372036854775807;
    m = m * 2;
    m += 1;                                    // u64 max, wrap-free
    s = fmt_u64_hex(a, m);
    expect(str_eq(s, "ffffffffffffffff"));
    free(a, s);
}

test "str_ends_with" {
    expect(str_ends_with("foobar", "bar"));
    expect(str_ends_with("foobar", "foobar"));
    expect(str_ends_with("foobar", ""));       // empty suffix
    expect(str_ends_with("", ""));
    expect(str_ends_with("a", "a"));
    expect(!str_ends_with("", "a"));
    expect(!str_ends_with("foo", "foobar"));   // suffix longer than s
    expect(!str_ends_with("foobar", "baz"));
    expect(!str_ends_with("foobar", "Bar"));   // case-sensitive
}

test "str_last_index_of" {
    expect(str_last_index_of("abcabc", 99) == 5);    // last 'c'
    expect(str_last_index_of("abcabc", 97) == 3);    // last 'a'
    expect(str_last_index_of("abcabc", 98) == 4);    // last 'b'
    expect(str_last_index_of("abc", 122) == 0 - 1);  // 'z' absent
    expect(str_last_index_of("", 97) == 0 - 1);      // empty
    expect(str_last_index_of("x", 120) == 0);        // single element
    // mirrors str_index_of: same answer when the byte is unique
    expect(str_last_index_of("hello world", 32) == str_index_of("hello world", 32));
}

test "str_count" {
    expect(str_count("banana", 97) == 3);   // 'a'
    expect(str_count("banana", 98) == 1);   // 'b'
    expect(str_count("banana", 122) == 0);  // 'z'
    expect(str_count("", 97) == 0);         // empty
    expect(str_count("aaaa", 97) == 4);     // all match
}

test "str_count concat property" {
    var a: Allocator = c_allocator();
    var x: []u8 = "mississippi";
    var y: []u8 = "kardashev";
    var both: []u8 = str_concat(a, x, y);
    expect(both.len == x.len + y.len);
    expect(str_count(both, 115) == str_count(x, 115) + str_count(y, 115)); // 's': 4+1
    expect(str_count(both, 105) == str_count(x, 105) + str_count(y, 105)); // 'i': 4+0
    expect(str_count(both, 115) == 5);
    expect(str_count(both, 105) == 4);
    free(a, both);
}

test "StrBuilder builds abc-123 newline" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    b.append(a, "abc");
    b.append_byte(a, 45);    // '-'
    b.append_i64(a, 123);
    b.append_byte(a, 10);    // '\n'
    expect(b.len() == 8);
    var s: []u8 = b.build(a);
    expect(str_eq(s, "abc-123\n"));
    expect(str_ends_with(s, "\n"));
    free(a, s);
    b.deinit(a);
}

test "StrBuilder negative and i64 min" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    b.append_i64(a, 0 - 42);
    b.append_byte(a, 44);    // ','
    b.append_i64(a, (0 - 9223372036854775807) - 1);
    expect(b.len() == 24);   // "-42," + 20-char i64 min
    var s: []u8 = b.build(a);
    expect(str_eq(s, "-42,-9223372036854775808"));
    free(a, s);
    b.deinit(a);
}

test "StrBuilder empty build" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    expect(b.len() == 0);
    var s: []u8 = b.build(a);
    expect(s.len == 0);
    expect(str_eq(s, ""));
    free(a, s);
    b.deinit(a);
}

test "StrBuilder growth past initial capacity" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    var i: i64 = 0;
    while (i < 50) : (i += 1) {
        b.append(a, "xy");
    }
    expect(b.len() == 100);    // way past the initial 8-byte capacity
    var s: []u8 = b.build(a);
    expect(s.len == 100);
    expect(s[0] == 120);       // 'x'
    expect(s[1] == 121);       // 'y'
    expect(s[98] == 120);
    expect(s[99] == 121);
    expect(str_count(s, 120) == 50);
    expect(str_count(s, 121) == 50);
    free(a, s);
    b.deinit(a);
}

test "StrBuilder long numeric string" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    var i: i64 = 0;
    while (i < 20) : (i += 1) {
        b.append_i64(a, i);
        b.append_byte(a, 44);  // ','
    }
    expect(b.len() == 50);     // 10*2 + 10*3 = 50 bytes
    var s: []u8 = b.build(a);
    expect(str_eq(s, "0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,"));
    free(a, s);
    b.deinit(a);
}
