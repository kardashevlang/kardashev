// tests/std/hashes.ks — std hashes module: crc32 / Crc32 (streaming) /
// fnv1a32 / fnv1a64 / adler32 / djb2.
//
// Every pinned value was computed independently with Python
// (zlib.crc32 / zlib.adler32 and hand-rolled FNV-1a / djb2 mod 2^32 / 2^64).
// fnv1a64 digests above the i64 literal range are compared via fmt_u64_hex.

@import("std");

test "crc32 known vectors" {
    expect(crc32("") == 0);
    expect(crc32("a") == 3904355907);
    expect(crc32("abc") == 891568578);
    expect(crc32("hello") == 907060870);
    expect(crc32("123456789") == 3421780262);    // 0xCBF43926, the IEEE check value
    expect(crc32("Wikipedia") == 2913648686);
    expect(crc32("The quick brown fox jumps over the lazy dog") == 1095738169);
}

test "crc32 is order-sensitive" {
    // same bytes, different order -> different digest (unlike a plain sum)
    expect(crc32("abc") != crc32("acb"));
    expect(crc32("abc") != crc32("cba"));
    // and a single flipped byte changes it
    expect(crc32("hello") != crc32("hellp"));
}

test "Crc32 streaming equals one-shot" {
    // no bytes fed -> the empty digest
    var h0: Crc32 = Crc32.init();
    expect(h0.final() == 0);
    expect(h0.final() == crc32(""));

    // one update == one-shot
    var h1: Crc32 = Crc32.init();
    h1.update("123456789");
    expect(h1.final() == 3421780262);

    // split updates == one-shot, empty updates are no-ops
    var h2: Crc32 = Crc32.init();
    h2.update("1234");
    h2.update("");
    h2.update("56789");
    expect(h2.final() == 3421780262);
    expect(h2.final() == crc32("123456789"));    // final() twice: state kept

    // byte-at-a-time == one-shot
    var s: []u8 = "Wikipedia";
    var h3: Crc32 = Crc32.init();
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        h3.update(s[i .. i + 1]);
    }
    expect(h3.final() == 2913648686);
    expect(h3.final() == crc32(s));
}

test "Crc32 update after final continues the stream" {
    var h: Crc32 = Crc32.init();
    h.update("1234");
    expect(h.final() == crc32("1234"));   // digest of the prefix
    h.update("56789");                    // keep feeding
    expect(h.final() == 3421780262);      // now the digest of the whole input
}

test "fnv1a32 known vectors" {
    expect(fnv1a32("") == 2166136261);    // the 32-bit offset basis
    expect(fnv1a32("a") == 3826002220);
    expect(fnv1a32("abc") == 440920331);
    expect(fnv1a32("hello") == 1335831723);
    expect(fnv1a32("123456789") == 3146166556);
    expect(fnv1a32("hello world") == 3582672807);
    expect(fnv1a32("The quick brown fox jumps over the lazy dog") == 76545936);
}

test "fnv1a32 single step hand-computed" {
    // one byte 'h' (104): (2166136261 ^ 104) * 16777619 mod 2^32
    expect(fnv1a32("h") == 3977000791);
    // order sensitivity
    expect(fnv1a32("ab") != fnv1a32("ba"));
}

test "fnv1a64 known vectors via hex" {
    // digests above the i64 literal range -> compare their hex rendering
    var a: Allocator = c_allocator();
    var s: []u8 = fmt_u64_hex(a, fnv1a64(""));
    expect(str_eq(s, "cbf29ce484222325"));       // the 64-bit offset basis
    free(a, s);
    s = fmt_u64_hex(a, fnv1a64("a"));
    expect(str_eq(s, "af63dc4c8601ec8c"));
    free(a, s);
    s = fmt_u64_hex(a, fnv1a64("abc"));
    expect(str_eq(s, "e71fa2190541574b"));
    free(a, s);
    s = fmt_u64_hex(a, fnv1a64("hello"));
    expect(str_eq(s, "a430d84680aabd0b"));
    free(a, s);
    s = fmt_u64_hex(a, fnv1a64("The quick brown fox jumps over the lazy dog"));
    expect(str_eq(s, "f3f9b7f5e7e47110"));
    free(a, s);
}

test "fnv1a64 value in i64 range compared directly" {
    // 0x06d5573923c6cdfc = 492395637191921148 fits i64, so == works
    expect(fnv1a64("123456789") == 492395637191921148);
    // and 32/64-bit FNV of the same input are unrelated values
    expect(fnv1a64("") != @as(u64, fnv1a32("")));
}

test "adler32 known vectors" {
    expect(adler32("") == 1);                    // s1=1, s2=0
    expect(adler32("a") == 6422626);             // (98 << 16) | 98
    expect(adler32("abc") == 38600999);
    expect(adler32("Wikipedia") == 300286872);   // 0x11E60398
    expect(adler32("hello world") == 436929629);
    expect(adler32("The quick brown fox jumps over the lazy dog") == 1541148634);
}

test "adler32 single-byte structure" {
    // one byte b: s1 = 1 + b, s2 = s1 -> ((1 + b) << 16) | (1 + b)
    expect(adler32("z") == 8061051);     // (123 << 16) | 123 = 8060928 + 123
    expect(adler32("A") == 4325442);     // (66 << 16) | 66 = 4325376 + 66
    // low half of any digest is s1 = (1 + byte sum) mod 65521
    expect((adler32("abc") & 65535) == 295);   // 1 + 97 + 98 + 99
}

test "djb2 known vectors" {
    expect(djb2("") == 5381);                    // the initial state
    expect(djb2("a") == 177670);                 // 5381 * 33 + 97
    expect(djb2("abc") == 193485963);
    expect(djb2("hello") == 261238937);
    expect(djb2("123456789") == 902675330);
    expect(djb2("Wikipedia") == 2567971580);
    expect(djb2("The quick brown fox jumps over the lazy dog") == 885799134);
}

test "djb2 two-byte hand computation" {
    // "ab": (5381 * 33 + 97) * 33 + 98 = 177670 * 33 + 98 = 5863208
    expect(djb2("ab") == 5863208);
    expect(djb2("ab") != djb2("ba"));    // order-sensitive
}

test "long input built by loop" {
    // 1000 bytes, byte i = 65 + (i % 26): 'A'..'Z' repeated
    var a: Allocator = c_allocator();
    var buf: []u8 = alloc(a, u8, 1000);
    var i: usize = 0;
    while (i < buf.len) : (i += 1) {
        buf[i] = @as(u8, 65 + (i % 26));
    }
    expect(crc32(buf) == 2677642917);
    expect(fnv1a32(buf) == 198892561);
    expect(adler32(buf) == 693055096);
    expect(djb2(buf) == 4169828397);
    var s: []u8 = fmt_u64_hex(a, fnv1a64(buf));
    expect(str_eq(s, "78f17fd98213ad31"));
    free(a, s);

    // streaming in 100-byte chunks == one-shot over the same buffer
    var h: Crc32 = Crc32.init();
    var off: usize = 0;
    while (off < buf.len) : (off += 100) {
        h.update(buf[off .. off + 100]);
    }
    expect(h.final() == 2677642917);
    free(a, buf);
}

test "all five digests disagree on a common input" {
    // sanity: the five functions are genuinely different hashes
    var v1: u32 = crc32("hello");
    var v2: u32 = fnv1a32("hello");
    var v3: u32 = adler32("hello");
    var v4: u32 = djb2("hello");
    expect(v1 != v2);
    expect(v1 != v3);
    expect(v1 != v4);
    expect(v2 != v3);
    expect(v2 != v4);
    expect(v3 != v4);
    expect(fnv1a64("hello") != @as(u64, v2));
}
