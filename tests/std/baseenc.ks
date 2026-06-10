// tests/std/baseenc.ks — std baseenc module: base64 + hex codecs
// (v0.157 std wave 2). Exercises every public fn: b64_encoded_len,
// b64_decoded_len, b64_encode, b64_decode, hex_encode, hex_decode.

@import("std");

test "b64_encoded_len" {
    expect(b64_encoded_len(0) == 0);
    expect(b64_encoded_len(1) == 4);     // 1 byte -> "xx=="
    expect(b64_encoded_len(2) == 4);
    expect(b64_encoded_len(3) == 4);
    expect(b64_encoded_len(4) == 8);
    expect(b64_encoded_len(5) == 8);
    expect(b64_encoded_len(6) == 8);
    expect(b64_encoded_len(7) == 12);
    expect(b64_encoded_len(256) == 344); // (256+2)/3*4 = 86*4
}

test "b64_decoded_len" {
    expect(b64_decoded_len("") == 0);
    expect(b64_decoded_len("Zg==") == 1);
    expect(b64_decoded_len("Zm8=") == 2);
    expect(b64_decoded_len("Zm9v") == 3);
    expect(b64_decoded_len("Zm9vYg==") == 4);
    expect(b64_decoded_len("Zm9vYmE=") == 5);
    expect(b64_decoded_len("Zm9vYmFy") == 6);
    expect(b64_decoded_len("abc") == 0);     // not a multiple of 4
    expect(b64_decoded_len("abcde") == 0);   // 5 % 4 != 0
}

test "b64_encode RFC 4648 vectors" {
    var a: Allocator = c_allocator();
    var s: []u8 = b64_encode(a, "");
    expect(s.len == 0);
    expect(str_eq(s, ""));
    free(a, s);
    s = b64_encode(a, "f");
    expect(str_eq(s, "Zg=="));
    free(a, s);
    s = b64_encode(a, "fo");
    expect(str_eq(s, "Zm8="));
    free(a, s);
    s = b64_encode(a, "foo");
    expect(str_eq(s, "Zm9v"));
    free(a, s);
    s = b64_encode(a, "foob");
    expect(str_eq(s, "Zm9vYg=="));
    free(a, s);
    s = b64_encode(a, "fooba");
    expect(str_eq(s, "Zm9vYmE="));
    free(a, s);
    s = b64_encode(a, "foobar");
    expect(str_eq(s, "Zm9vYmFy"));
    free(a, s);
}

test "b64_decode RFC 4648 vectors" {
    var a: Allocator = c_allocator();
    var d: []u8 = b64_decode(a, "");
    expect(d.len == 0);                      // "" decodes to "" (success)
    free(a, d);
    d = b64_decode(a, "Zg==");
    expect(str_eq(d, "f"));
    free(a, d);
    d = b64_decode(a, "Zm8=");
    expect(str_eq(d, "fo"));
    free(a, d);
    d = b64_decode(a, "Zm9v");
    expect(str_eq(d, "foo"));
    free(a, d);
    d = b64_decode(a, "Zm9vYg==");
    expect(str_eq(d, "foob"));
    free(a, d);
    d = b64_decode(a, "Zm9vYmE=");
    expect(str_eq(d, "fooba"));
    free(a, d);
    d = b64_decode(a, "Zm9vYmFy");
    expect(str_eq(d, "foobar"));
    free(a, d);
    // The other classic vectors (Wikipedia's "Man"):
    d = b64_decode(a, "TWFu");
    expect(str_eq(d, "Man"));
    free(a, d);
    d = b64_decode(a, "TWE=");
    expect(str_eq(d, "Ma"));
    free(a, d);
    d = b64_decode(a, "TQ==");
    expect(str_eq(d, "M"));
    free(a, d);
}

test "b64 multi-group sentence round-trip" {
    var a: Allocator = c_allocator();
    var s: []u8 = b64_encode(a, "Hello, World!");
    expect(str_eq(s, "SGVsbG8sIFdvcmxkIQ=="));
    expect(s.len == b64_encoded_len(13));    // 13 bytes -> 20 chars
    expect(b64_decoded_len(s) == 13);
    var d: []u8 = b64_decode(a, s);
    expect(str_eq(d, "Hello, World!"));
    free(a, d);
    free(a, s);
}

test "b64 alphabet extremes: '+' and '/'" {
    var a: Allocator = c_allocator();
    var hi: [3]u8 = [3]u8{ 255, 255, 255 };  // 0xFF FF FF -> four 63s
    var s: []u8 = b64_encode(a, hi[0..3]);
    expect(str_eq(s, "////"));
    free(a, s);
    var one: [1]u8 = [1]u8{248};             // 0xF8: 111110|00.. -> '+', 'A'
    s = b64_encode(a, one[0..1]);
    expect(str_eq(s, "+A=="));
    free(a, s);
    var d: []u8 = b64_decode(a, "+A==");
    expect(d.len == 1);
    expect(d[0] == 248);
    free(a, d);
    d = b64_decode(a, "////");
    expect(d.len == 3);
    expect(d[0] == 255 and d[1] == 255 and d[2] == 255);
    free(a, d);
}

test "b64_decode rejects invalid input" {
    var a: Allocator = c_allocator();
    var d: []u8 = b64_decode(a, "Z");        // length 1: not a multiple of 4
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zg");                 // length 2
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zg=");                // length 3
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zm9vY");              // length 5
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zm9$");               // '$' not in the alphabet
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zm9-");               // url-safe '-' is rejected
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zm9_");               // url-safe '_' is rejected
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zm9\n");              // whitespace is rejected
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "====");               // padding only
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "a===");               // three padding chars
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "ab=c");               // '=' not in final position
    expect(d.len == 0);
    free(a, d);
    d = b64_decode(a, "Zg==Zg==");           // '=' mid-text (two blocks)
    expect(d.len == 0);
    free(a, d);
}

test "b64 round-trip all 256 byte values" {
    var a: Allocator = c_allocator();
    var data: []u8 = alloc(a, u8, 256);
    var i: usize = 0;
    while (i < 256) : (i += 1) {
        data[i] = @as(u8, i);
    }
    var enc: []u8 = b64_encode(a, data);
    expect(enc.len == b64_encoded_len(256));
    expect(enc.len == 344);
    expect(str_starts_with(enc, "AAECAwQFBgcICQoL"));  // bytes 0..11
    expect(str_ends_with(enc, "/P3+/w=="));            // bytes 252..255 + pad
    expect(b64_decoded_len(enc) == 256);
    var dec: []u8 = b64_decode(a, enc);
    expect(dec.len == 256);
    var ok: bool = true;
    var j: usize = 0;
    while (j < dec.len) : (j += 1) {
        if (dec[j] != data[j]) {
            ok = false;
        }
    }
    expect(ok);
    free(a, dec);
    free(a, enc);
    free(a, data);
}

test "b64 re-encode is identity on canonical text" {
    var a: Allocator = c_allocator();
    var d: []u8 = b64_decode(a, "Zm9vYmE=");
    var s: []u8 = b64_encode(a, d);
    expect(str_eq(s, "Zm9vYmE="));           // encode(decode(x)) == x
    free(a, s);
    free(a, d);
}

test "hex_encode basics" {
    var a: Allocator = c_allocator();
    var s: []u8 = hex_encode(a, "");
    expect(s.len == 0);
    free(a, s);
    s = hex_encode(a, "f");
    expect(str_eq(s, "66"));                 // 'f' == 0x66
    free(a, s);
    s = hex_encode(a, "foobar");
    expect(str_eq(s, "666f6f626172"));
    free(a, s);
    var b: [5]u8 = [5]u8{ 0, 1, 15, 16, 255 };
    s = hex_encode(a, b[0..5]);
    expect(str_eq(s, "00010f10ff"));         // leading zeros kept, lowercase
    free(a, s);
}

test "hex_decode lower, upper and mixed case" {
    var a: Allocator = c_allocator();
    var d: []u8 = hex_decode(a, "666f6f626172");
    expect(str_eq(d, "foobar"));
    free(a, d);
    d = hex_decode(a, "deadbeef");
    expect(d.len == 4);
    expect(d[0] == 222 and d[1] == 173 and d[2] == 190 and d[3] == 239);
    free(a, d);
    d = hex_decode(a, "DEADBEEF");           // upper-case accepted
    expect(d.len == 4);
    expect(d[0] == 222 and d[1] == 173 and d[2] == 190 and d[3] == 239);
    free(a, d);
    d = hex_decode(a, "DeAdBeEf");           // mixed-case accepted
    expect(d.len == 4);
    expect(d[0] == 222 and d[1] == 173 and d[2] == 190 and d[3] == 239);
    free(a, d);
    d = hex_decode(a, "00ff");
    expect(d.len == 2);
    expect(d[0] == 0 and d[1] == 255);
    free(a, d);
    d = hex_decode(a, "");                   // "" decodes to "" (success)
    expect(d.len == 0);
    free(a, d);
    // Case-insensitivity as a property: both spellings, same bytes.
    var lo: []u8 = hex_decode(a, "abcdef");
    var up: []u8 = hex_decode(a, "ABCDEF");
    expect(str_eq(lo, up));
    expect(lo[0] == 171 and lo[1] == 205 and lo[2] == 239);
    free(a, up);
    free(a, lo);
}

test "hex_decode rejects invalid input" {
    var a: Allocator = c_allocator();
    var d: []u8 = hex_decode(a, "f");        // odd length
    expect(d.len == 0);
    free(a, d);
    d = hex_decode(a, "abc");                // odd length
    expect(d.len == 0);
    free(a, d);
    d = hex_decode(a, "0g");                 // 'g' is not hex
    expect(d.len == 0);
    free(a, d);
    d = hex_decode(a, "0x12");               // 'x' prefix is not hex
    expect(d.len == 0);
    free(a, d);
    d = hex_decode(a, " 12");                // leading space
    expect(d.len == 0);
    free(a, d);
    d = hex_decode(a, "12 ");                // trailing space
    expect(d.len == 0);
    free(a, d);
    d = hex_decode(a, "-1");                 // sign is not hex
    expect(d.len == 0);
    free(a, d);
}

test "hex round-trip all 256 byte values" {
    var a: Allocator = c_allocator();
    var data: []u8 = alloc(a, u8, 256);
    var i: usize = 0;
    while (i < 256) : (i += 1) {
        data[i] = @as(u8, i);
    }
    var enc: []u8 = hex_encode(a, data);
    expect(enc.len == 512);
    expect(str_starts_with(enc, "000102030405"));
    expect(str_ends_with(enc, "fafbfcfdfeff"));
    var dec: []u8 = hex_decode(a, enc);
    expect(dec.len == 256);
    var ok: bool = true;
    var j: usize = 0;
    while (j < dec.len) : (j += 1) {
        if (dec[j] != data[j]) {
            ok = false;
        }
    }
    expect(ok);
    free(a, dec);
    free(a, enc);
    free(a, data);
}

test "hex re-encode is identity on canonical lowercase text" {
    var a: Allocator = c_allocator();
    var d: []u8 = hex_decode(a, "0123456789abcdef");
    expect(d.len == 8);
    expect(d[0] == 1);                       // 0x01
    expect(d[7] == 239);                     // 0xef
    var s: []u8 = hex_encode(a, d);
    expect(str_eq(s, "0123456789abcdef"));   // encode(decode(x)) == x
    free(a, s);
    free(a, d);
}
