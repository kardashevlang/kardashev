// tests/std/numtext.ks — std numtext module: parse_f64 / fmt_f64,
// parse_u64 / fmt_u64 / fmt_i64_pad, to_lower / to_upper / eq_ignore_case.
//
// Float pins were computed by simulating the exact double arithmetic of the
// implementation (digit accumulation, exact-power-of-10 scaling, fraction
// rounding via trunc(fp * 10^d + 0.5)) — binary-representation effects like
// fmt_f64(2.675, 2) == "2.67" are pinned honestly.

@import("std");

test "parse_f64 basics" {
    expect((parse_f64("0") orelse 999.0) == 0.0);
    expect((parse_f64("0.5") orelse 999.0) == 0.5);
    expect((parse_f64("-3.25") orelse 999.0) == 0.0 - 3.25);
    expect((parse_f64("+2.5") orelse 999.0) == 2.5);
    expect((parse_f64("42") orelse 999.0) == 42.0);
    expect((parse_f64("5.") orelse 999.0) == 5.0);          // trailing dot OK
    expect((parse_f64(".5") orelse 999.0) == 0.5);          // leading dot OK
    expect((parse_f64("-0") orelse 999.0) == 0.0);          // -0.0 == 0.0 (IEEE)
    expect((parse_f64("123456789.0") orelse 999.0) == 123456789.0);
    expect((parse_f64("0042.50") orelse 999.0) == 42.5);    // leading zeros OK
}

test "parse_f64 exponents" {
    expect((parse_f64("1e3") orelse 999.0) == 1000.0);
    expect((parse_f64("1E2") orelse 999.0) == 100.0);
    expect((parse_f64("1.5e2") orelse 999.0) == 150.0);
    expect((parse_f64("2.5e-2") orelse 999.0) == 0.025);    // 25/1000, correctly rounded
    expect((parse_f64("2.5E+2") orelse 999.0) == 250.0);
    expect((parse_f64("1e-2") orelse 999.0) == 0.01);
    expect((parse_f64("1e0") orelse 999.0) == 1.0);
    expect((parse_f64("12345.6789e1") orelse 999.0) == 123456.789);
    expect((parse_f64("1e02") orelse 999.0) == 100.0);      // exponent leading zero
}

test "parse_f64 rejects garbage" {
    expect((parse_f64("") orelse 11.0) == 11.0);            // empty
    expect((parse_f64(".") orelse 22.0) == 22.0);           // dot alone
    expect((parse_f64("-") orelse 33.0) == 33.0);           // sign alone
    expect((parse_f64("+") orelse 44.0) == 44.0);
    expect((parse_f64("e3") orelse 55.0) == 55.0);          // no mantissa digit
    expect((parse_f64("1e") orelse 66.0) == 66.0);          // no exponent digit
    expect((parse_f64("1e+") orelse 77.0) == 77.0);
    expect((parse_f64("1.2.3") orelse 88.0) == 88.0);       // second dot
    expect((parse_f64("1x") orelse 99.0) == 99.0);          // trailing letter
    expect((parse_f64(" 1") orelse 12.0) == 12.0);          // leading space
    expect((parse_f64("1 ") orelse 13.0) == 13.0);          // trailing space
    expect((parse_f64("--1") orelse 14.0) == 14.0);         // double sign
    expect((parse_f64("+-1") orelse 15.0) == 15.0);
    expect((parse_f64("1e2.5") orelse 16.0) == 16.0);       // dot after exponent
    expect((parse_f64("nan") orelse 17.0) == 17.0);         // names not parsed
    expect((parse_f64("inf") orelse 18.0) == 18.0);
}

test "parse_f64 overflow underflow and long mantissa" {
    // 1e400 overflows to +inf: positive and x/2 == x only holds for inf.
    var big: f64 = parse_f64("1e400") orelse 0.0;
    expect(big > 0.0);
    expect(big / 2.0 == big);
    var nbig: f64 = parse_f64("-1e400") orelse 0.0;
    expect(nbig < 0.0);
    expect(nbig / 2.0 == nbig);
    // 1e-400 underflows to zero; 0e999 never scales (no 0 * inf NaN).
    expect((parse_f64("1e-400") orelse 999.0) == 0.0);
    expect((parse_f64("0e999") orelse 999.0) == 0.0);
    expect((parse_f64("-0e999") orelse 999.0) == 0.0);
    // A 30-digit mantissa accumulates with f64 rounding but lands in
    // (10^29, 10^30): build the bound by repeated multiplication.
    var m: f64 = parse_f64("123456789012345678901234567890") orelse 0.0;
    var p29: f64 = 1.0;
    var i: i64 = 0;
    while (i < 29) : (i += 1) {
        p29 = p29 * 10.0;
    }
    expect(m > p29);
    expect(m < p29 * 10.0);
}

test "fmt_f64 basics" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_f64(a, 0.0, 0);
    expect(str_eq(s, "0"));
    free(a, s);
    s = fmt_f64(a, 0.0, 2);
    expect(str_eq(s, "0.00"));
    free(a, s);
    s = fmt_f64(a, 0.5, 1);
    expect(str_eq(s, "0.5"));
    free(a, s);
    s = fmt_f64(a, 0.5, 3);
    expect(str_eq(s, "0.500"));
    free(a, s);
    s = fmt_f64(a, 42.0, 0);
    expect(str_eq(s, "42"));
    free(a, s);
    s = fmt_f64(a, 0.0 - 7.0, 3);
    expect(str_eq(s, "-7.000"));
    free(a, s);
    s = fmt_f64(a, 1000.0, 1);
    expect(str_eq(s, "1000.0"));
    free(a, s);
    s = fmt_f64(a, 123456.789, 3);
    expect(str_eq(s, "123456.789"));
    free(a, s);
    // -0.0 renders unsigned (v < 0.0 is false for negative zero)
    var nz: f64 = (0.0 - 1.0) * 0.0;
    s = fmt_f64(a, nz, 1);
    expect(str_eq(s, "0.0"));
    free(a, s);
}

test "fmt_f64 rounding" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_f64(a, 0.5, 0);
    expect(str_eq(s, "1"));                      // half-up at 0 decimals
    free(a, s);
    s = fmt_f64(a, 0.0 - 3.25, 1);
    expect(str_eq(s, "-3.3"));                   // 0.25 is exact binary: half-up
    free(a, s);
    s = fmt_f64(a, 2.675, 2);
    expect(str_eq(s, "2.67"));                   // double(2.675) = 2.67499999...
    free(a, s);
    s = fmt_f64(a, 0.025, 2);
    expect(str_eq(s, "0.03"));                   // fp*100 lands at 2.5000...04
    free(a, s);
    s = fmt_f64(a, 9.999, 2);
    expect(str_eq(s, "10.00"));                  // fraction carry into integer
    free(a, s);
    s = fmt_f64(a, 0.005, 2);
    expect(str_eq(s, "0.01"));
    free(a, s);
    s = fmt_f64(a, 0.0 - 0.005, 2);
    expect(str_eq(s, "-0.01"));
    free(a, s);
    s = fmt_f64(a, 0.1, 1);
    expect(str_eq(s, "0.1"));
    free(a, s);
}

test "fmt_f64 decimals clamp" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_f64(a, 1.5, 0 - 3);
    expect(str_eq(s, "2"));                      // negative clamps to 0, rounds
    free(a, s);
    s = fmt_f64(a, 0.5, 25);
    expect(str_eq(s, "0.50000000000000000"));    // clamps to 17 digits
    free(a, s);
    s = fmt_f64(a, 1.0, 17);
    expect(str_eq(s, "1.00000000000000000"));
    free(a, s);
    s = fmt_f64(a, 0.1, 17);
    expect(str_eq(s, "0.10000000000000000"));    // fp*1e17 rounds to 1e16 exactly
    free(a, s);
}

test "fmt_f64 big and specials" {
    var a: Allocator = c_allocator();
    // 2^70 and 2^100, built by exact doubling, exercise the >= 2^63 path.
    var v: f64 = 1.0;
    var i: i64 = 0;
    while (i < 70) : (i += 1) {
        v = v * 2.0;
    }
    var s: []u8 = fmt_f64(a, v, 0);
    expect(str_eq(s, "1180591620717411303424"));
    free(a, s);
    s = fmt_f64(a, v, 2);
    expect(str_eq(s, "1180591620717411303424.00"));
    free(a, s);
    s = fmt_f64(a, 0.0 - v, 0);
    expect(str_eq(s, "-1180591620717411303424"));
    free(a, s);
    while (i < 100) : (i += 1) {
        v = v * 2.0;
    }
    s = fmt_f64(a, v, 0);
    expect(str_eq(s, "1267650600228229401496703205376"));
    free(a, s);
    // the 22-digit 2^70 decimal parses back to exactly 2^70
    var back: f64 = parse_f64("1180591620717411303424") orelse 0.0;
    var p70: f64 = 1.0;
    var j: i64 = 0;
    while (j < 70) : (j += 1) {
        p70 = p70 * 2.0;
    }
    expect(back == p70);
    // specials: inf, -inf, nan
    var g: f64 = 1.0;
    var k: i64 = 0;
    while (k < 400) : (k += 1) {
        g = g * 10.0;
    }
    s = fmt_f64(a, g, 2);
    expect(str_eq(s, "inf"));
    free(a, s);
    s = fmt_f64(a, 0.0 - g, 0);
    expect(str_eq(s, "-inf"));
    free(a, s);
    var z: f64 = 0.0;
    var nanv: f64 = z / z;
    s = fmt_f64(a, nanv, 5);
    expect(str_eq(s, "nan"));
    free(a, s);
}

test "parse_f64 fmt_f64 round-trip on exact values" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_f64(a, 0.5, 3);
    expect((parse_f64(s) orelse 999.0) == 0.5);
    free(a, s);
    s = fmt_f64(a, 0.0 - 3.25, 2);
    expect((parse_f64(s) orelse 999.0) == 0.0 - 3.25);
    free(a, s);
    s = fmt_f64(a, 1000.0, 1);
    expect((parse_f64(s) orelse 999.0) == 1000.0);
    free(a, s);
    s = fmt_f64(a, 42.0, 0);
    expect((parse_f64(s) orelse 999.0) == 42.0);
    free(a, s);
    s = fmt_f64(a, 0.025, 3);
    expect((parse_f64(s) orelse 999.0) == 0.025);
    free(a, s);
}

test "parse_u64 basics and boundaries" {
    expect((parse_u64("0") orelse 7) == 0);
    expect((parse_u64("7") orelse 0) == 7);
    expect((parse_u64("42") orelse 0) == 42);
    expect((parse_u64("0042") orelse 0) == 42);             // leading zeros OK
    expect((parse_u64("9223372036854775807") orelse 0) == 9223372036854775807);
    var mx: u64 = 0;
    mx -= 1;                                                // u64 max, wrap-defined
    expect((parse_u64("18446744073709551615") orelse 0) == mx);
    expect((parse_u64("18446744073709551616") orelse 1) == 1);   // max+1 -> null
    expect((parse_u64("18446744073709551625") orelse 2) == 2);   // last-digit branch
    expect((parse_u64("20000000000000000000") orelse 3) == 3);   // limb branch
    expect((parse_u64("99999999999999999999") orelse 4) == 4);
    expect((parse_u64("184467440737095516150") orelse 5) == 5);  // 21 digits
}

test "parse_u64 rejects garbage" {
    expect((parse_u64("") orelse 11) == 11);                // empty
    expect((parse_u64("-1") orelse 22) == 22);              // no sign
    expect((parse_u64("+1") orelse 33) == 33);
    expect((parse_u64("12a") orelse 44) == 44);             // embedded letter
    expect((parse_u64(" 1") orelse 55) == 55);              // leading space
    expect((parse_u64("1 ") orelse 66) == 66);              // trailing space
    expect((parse_u64("1.5") orelse 77) == 77);             // not an integer
}

test "fmt_u64" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_u64(a, 0);
    expect(str_eq(s, "0"));
    free(a, s);
    s = fmt_u64(a, 7);
    expect(str_eq(s, "7"));
    free(a, s);
    s = fmt_u64(a, 1000);
    expect(str_eq(s, "1000"));
    free(a, s);
    s = fmt_u64(a, 1234567890123456789);
    expect(str_eq(s, "1234567890123456789"));
    free(a, s);
    var mx: u64 = 0;
    mx -= 1;
    s = fmt_u64(a, mx);
    expect(str_eq(s, "18446744073709551615"));              // full u64 range
    free(a, s);
}

test "fmt_u64 parse_u64 round-trip property" {
    var a: Allocator = c_allocator();
    var mx: u64 = 0;
    mx -= 1;
    var vals: [6]u64 = [6]u64{ 0, 1, 10, 4294967296, 9223372036854775807, 0 };
    vals[5] = mx;
    for (vals) |v| {
        var s: []u8 = fmt_u64(a, v);
        expect((parse_u64(s) orelse 424242) == v);          // sentinel not in vals
        free(a, s);
    }
}

test "fmt_i64_pad matrix" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_i64_pad(a, 42, 5, false);
    expect(str_eq(s, "   42"));
    free(a, s);
    s = fmt_i64_pad(a, 42, 5, true);
    expect(str_eq(s, "00042"));
    free(a, s);
    s = fmt_i64_pad(a, 0 - 42, 6, false);
    expect(str_eq(s, "   -42"));
    free(a, s);
    s = fmt_i64_pad(a, 0 - 42, 6, true);
    expect(str_eq(s, "-00042"));                            // zeros after the sign
    free(a, s);
    s = fmt_i64_pad(a, 0, 3, true);
    expect(str_eq(s, "000"));
    free(a, s);
    s = fmt_i64_pad(a, 0, 3, false);
    expect(str_eq(s, "  0"));
    free(a, s);
    s = fmt_i64_pad(a, 0 - 7, 3, true);
    expect(str_eq(s, "-07"));
    free(a, s);
    s = fmt_i64_pad(a, 0 - 7, 3, false);
    expect(str_eq(s, " -7"));
    free(a, s);
}

test "fmt_i64_pad width at or below rendering" {
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_i64_pad(a, 42, 2, true);              // exact width
    expect(str_eq(s, "42"));
    free(a, s);
    s = fmt_i64_pad(a, 42, 1, false);                       // narrower than body
    expect(str_eq(s, "42"));
    free(a, s);
    s = fmt_i64_pad(a, 0 - 7, 2, true);                     // sign counts
    expect(str_eq(s, "-7"));
    free(a, s);
    s = fmt_i64_pad(a, 5, 0, true);                         // zero width
    expect(str_eq(s, "5"));
    free(a, s);
    s = fmt_i64_pad(a, 5, 0 - 3, false);                    // negative width
    expect(str_eq(s, "5"));
    free(a, s);
}

test "fmt_i64_pad i64 extremes" {
    var a: Allocator = c_allocator();
    var mn: i64 = (0 - 9223372036854775807) - 1;
    var s: []u8 = fmt_i64_pad(a, mn, 25, true);
    expect(str_eq(s, "-000009223372036854775808"));         // 25 bytes
    free(a, s);
    s = fmt_i64_pad(a, mn, 25, false);
    expect(str_eq(s, "     -9223372036854775808"));
    free(a, s);
    s = fmt_i64_pad(a, 9223372036854775807, 21, true);
    expect(str_eq(s, "009223372036854775807"));
    free(a, s);
}

test "to_lower and to_upper" {
    var a: Allocator = c_allocator();
    var s: []u8 = to_lower(a, "Hello, World! 123");
    expect(str_eq(s, "hello, world! 123"));
    free(a, s);
    s = to_upper(a, "Hello, World! 123");
    expect(str_eq(s, "HELLO, WORLD! 123"));
    free(a, s);
    s = to_lower(a, "");
    expect(str_eq(s, ""));
    free(a, s);
    s = to_upper(a, "");
    expect(str_eq(s, ""));
    free(a, s);
    // ASCII boundary bytes: '@'(64) 'A' 'Z' '['(91) '`'(96) 'a' 'z' '{'(123) —
    // only the letters move, the neighbours stay put.
    s = to_lower(a, "@AZ[`az{");
    expect(str_eq(s, "@az[`az{"));
    free(a, s);
    s = to_upper(a, "@AZ[`az{");
    expect(str_eq(s, "@AZ[`AZ{"));
    free(a, s);
}

test "to_lower to_upper composition property" {
    var a: Allocator = c_allocator();
    var src: []u8 = "MiXeD 123 sTr!";
    var up: []u8 = to_upper(a, src);
    expect(str_eq(up, "MIXED 123 STR!"));
    var lo1: []u8 = to_lower(a, up);                        // lower(upper(s))
    var lo2: []u8 = to_lower(a, src);                       // == lower(s)
    expect(str_eq(lo1, lo2));
    expect(str_eq(lo1, "mixed 123 str!"));
    expect(eq_ignore_case(src, up));                        // and case-blind equal
    free(a, up);
    free(a, lo1);
    free(a, lo2);
}

test "eq_ignore_case" {
    expect(eq_ignore_case("Hello", "hELLO"));
    expect(eq_ignore_case("abc", "abc"));
    expect(eq_ignore_case("ABC", "abc"));
    expect(eq_ignore_case("a_b", "A_B"));
    expect(eq_ignore_case("", ""));
    expect(eq_ignore_case("KARDASHEV", "kardashev"));
    expect(!eq_ignore_case("abc", "abd"));                  // different letter
    expect(!eq_ignore_case("abc", "abcd"));                 // different length
    expect(!eq_ignore_case("abc", ""));
    // '@'(64)/'`'(96) and '['(91)/'{'(123) differ by 32 but are NOT letters:
    // folding must leave them apart.
    expect(!eq_ignore_case("a@b", "a`b"));
    expect(!eq_ignore_case("[", "{"));
}
