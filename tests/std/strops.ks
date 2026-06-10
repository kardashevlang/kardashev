@import("std");

// tests/std/strops.ks — std strops module: SpanPair, Splitter / StrSplitter,
// split_init / split_init_str / split_collect, trim / trim_start / trim_end,
// join, replace. Every expectation below is a hand-computed value; string
// results are compared with str_eq.

test "SpanPair names a subslice of its source" {
    var p: SpanPair = SpanPair{ .off = 6, .len = 5 };
    expect(p.off == 6);
    expect(p.len == 5);
    var s: []u8 = "hello world";
    expect(str_eq(s[p.off..p.off + p.len], "world"));
    var z: SpanPair = SpanPair{ .off = 0, .len = 0 };
    expect(str_eq(s[z.off..z.off + z.len], ""));
}

test "split_init: fields in order, empty field between consecutive seps" {
    var it: Splitter = split_init("a,bb,,c", 44);   // ','
    expect(it.next());
    expect(str_eq(it.current(), "a"));
    expect(it.next());
    expect(str_eq(it.current(), "bb"));
    expect(it.next());
    expect(str_eq(it.current(), ""));               // between ",," — yielded
    expect(it.next());
    expect(str_eq(it.current(), "c"));
    expect(!it.next());
    expect(!it.next());                             // stays exhausted
}

test "split_init: leading and trailing separators yield empty fields" {
    // ",a," -> "", "a", ""  (2 separators -> 3 fields)
    var it: Splitter = split_init(",a,", 44);
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(it.next());
    expect(str_eq(it.current(), "a"));
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(!it.next());

    // ",," -> three empty fields
    var it2: Splitter = split_init(",,", 44);
    var n: i64 = 0;
    while (it2.next()) {
        expect(it2.current().len == 0);
        n += 1;
    }
    expect(n == 3);
}

test "split_init: empty string yields exactly one empty field" {
    var it: Splitter = split_init("", 44);
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(it.current().len == 0);
    expect(!it.next());
}

test "split_init: separator absent yields the whole string" {
    var it: Splitter = split_init("hello", 44);
    expect(it.next());
    expect(str_eq(it.current(), "hello"));
    expect(!it.next());
}

test "split_init: field count is separator count + 1" {
    var s: []u8 = "a,b,,c,";
    var fields: i64 = 0;
    var it: Splitter = split_init(s, 44);
    while (it.next()) {
        fields += 1;
    }
    expect(str_count(s, 44) == 4);
    expect(fields == 5);
}

test "split_init_str: multi-byte separator" {
    var it: StrSplitter = split_init_str("a==bb==c", "==");
    expect(it.next());
    expect(str_eq(it.current(), "a"));
    expect(it.next());
    expect(str_eq(it.current(), "bb"));
    expect(it.next());
    expect(str_eq(it.current(), "c"));
    expect(!it.next());
    expect(!it.next());                             // stays exhausted
}

test "split_init_str: consecutive separators yield an empty field" {
    // "x====y" on "==" -> "x", "", "y"
    var it: StrSplitter = split_init_str("x====y", "==");
    expect(it.next());
    expect(str_eq(it.current(), "x"));
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(it.next());
    expect(str_eq(it.current(), "y"));
    expect(!it.next());
}

test "split_init_str: leading and trailing separators" {
    // "::a::b::" on "::" -> "", "a", "b", ""
    var it: StrSplitter = split_init_str("::a::b::", "::");
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(it.next());
    expect(str_eq(it.current(), "a"));
    expect(it.next());
    expect(str_eq(it.current(), "b"));
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(!it.next());
}

test "split_init_str: matches are non-overlapping, left-to-right" {
    // "aaa" on "aa": match at 0, scan resumes at 2 -> "", "a"
    var it: StrSplitter = split_init_str("aaa", "aa");
    expect(it.next());
    expect(str_eq(it.current(), ""));
    expect(it.next());
    expect(str_eq(it.current(), "a"));
    expect(!it.next());

    // "aaaa" on "aa": matches at 0 and 2 -> "", "", ""
    var it2: StrSplitter = split_init_str("aaaa", "aa");
    var n: i64 = 0;
    while (it2.next()) {
        expect(it2.current().len == 0);
        n += 1;
    }
    expect(n == 3);
}

test "split_init_str: separator absent or longer than the string" {
    var it: StrSplitter = split_init_str("abc", "xy");
    expect(it.next());
    expect(str_eq(it.current(), "abc"));
    expect(!it.next());

    var it2: StrSplitter = split_init_str("ab", "abc");
    expect(it2.next());
    expect(str_eq(it2.current(), "ab"));
    expect(!it2.next());
}

test "split_init_str: empty separator yields the whole string once" {
    var it: StrSplitter = split_init_str("abc", "");
    expect(it.next());
    expect(str_eq(it.current(), "abc"));
    expect(!it.next());

    var it2: StrSplitter = split_init_str("", "");
    expect(it2.next());
    expect(str_eq(it2.current(), ""));
    expect(!it2.next());

    var it3: StrSplitter = split_init_str("", "x");
    expect(it3.next());
    expect(str_eq(it3.current(), ""));
    expect(!it3.next());
}

test "split_collect: spans recover every field" {
    var a: Allocator = c_allocator();
    var s: []u8 = "one,two,,three";
    // Bytes: one=0..3, ','=3, two=4..7, ','=7, ','=8, three=9..14.
    var parts: ArrayList(SpanPair) = split_collect(a, s, 44);
    expect(parts.len() == 4);
    var p0: SpanPair = parts.get(0);
    expect(p0.off == 0);
    expect(p0.len == 3);
    expect(str_eq(s[p0.off..p0.off + p0.len], "one"));
    var p1: SpanPair = parts.get(1);
    expect(p1.off == 4);
    expect(p1.len == 3);
    expect(str_eq(s[p1.off..p1.off + p1.len], "two"));
    var p2: SpanPair = parts.get(2);
    expect(p2.off == 8);
    expect(p2.len == 0);
    var p3: SpanPair = parts.get(3);
    expect(p3.off == 9);
    expect(p3.len == 5);
    expect(str_eq(s[p3.off..p3.off + p3.len], "three"));
    parts.deinit(a);
}

test "split_collect: empty string gives one zero-length span" {
    var a: Allocator = c_allocator();
    var parts: ArrayList(SpanPair) = split_collect(a, "", 44);
    expect(parts.len() == 1);
    expect(parts.get(0).off == 0);
    expect(parts.get(0).len == 0);
    parts.deinit(a);
}

test "trim_start matrix" {
    expect(str_eq(trim_start("   hi"), "hi"));
    expect(str_eq(trim_start("hi   "), "hi   "));     // trailing kept
    expect(str_eq(trim_start("\t\n hi"), "hi"));
    expect(str_eq(trim_start(""), ""));
    expect(str_eq(trim_start("   "), ""));            // all whitespace
    expect(str_eq(trim_start("a b"), "a b"));         // nothing to trim
    expect(trim_start(" ab").len == 2);
}

test "trim_end matrix" {
    expect(str_eq(trim_end("hi   "), "hi"));
    expect(str_eq(trim_end("   hi"), "   hi"));       // leading kept
    expect(str_eq(trim_end("hi \t\n"), "hi"));
    expect(str_eq(trim_end(""), ""));
    expect(str_eq(trim_end("\n\n"), ""));             // all whitespace
    expect(str_eq(trim_end("a b"), "a b"));           // nothing to trim
    expect(trim_end("ab ").len == 2);
}

test "trim trims both ends, keeps interior whitespace" {
    expect(str_eq(trim(" \t a b \n "), "a b"));
    expect(str_eq(trim("abc"), "abc"));
    expect(str_eq(trim(""), ""));
    expect(str_eq(trim(" "), ""));
    expect(str_eq(trim("\t\n \n\t"), ""));
    expect(str_eq(trim("x"), "x"));
    expect(str_eq(trim("  x  "), "x"));
}

test "trim handles CR bytes (no \\r escape, so build the bytes)" {
    var arr: [5]u8 = [5]u8{ 13, 32, 120, 9, 13 };   // CR SP 'x' TAB CR
    var s: []u8 = arr[0..5];
    expect(str_eq(trim(s), "x"));
    var ts: []u8 = trim_start(s);                   // 'x' TAB CR
    expect(ts.len == 3);
    expect(ts[0] == 120);
    var te: []u8 = trim_end(s);                     // CR SP 'x'
    expect(te.len == 3);
    expect(te[2] == 120);
}

test "trim results are zero-copy views into the source" {
    var arr: [4]u8 = [4]u8{ 32, 32, 97, 98 };       // "  ab"
    var s: []u8 = arr[0..4];
    var t: []u8 = trim_start(s);
    expect(t.len == 2);
    expect(t[0] == 97);                             // 'a'
    arr[2] = 99;                                    // mutate the source: 'a' -> 'c'
    expect(t[0] == 99);                             // the view sees it — no copy
}

test "join: basics" {
    var a: Allocator = c_allocator();
    var s: []u8 = "a,b,c";
    var parts: ArrayList(SpanPair) = split_collect(a, s, 44);
    expect(parts.len() == 3);

    var j1: []u8 = join(a, s, parts, ",");
    expect(str_eq(j1, "a,b,c"));
    free(a, j1);

    var j2: []u8 = join(a, s, parts, " -- ");       // multi-byte separator
    expect(str_eq(j2, "a -- b -- c"));
    free(a, j2);

    var j3: []u8 = join(a, s, parts, "");           // empty separator
    expect(str_eq(j3, "abc"));
    free(a, j3);
    parts.deinit(a);
}

test "join: empty list and single field" {
    var a: Allocator = c_allocator();
    var none: ArrayList(SpanPair) = ArrayList(SpanPair).init(a);
    var j0: []u8 = join(a, "whatever", none, ",");
    expect(str_eq(j0, ""));
    expect(j0.len == 0);
    free(a, j0);
    none.deinit(a);

    var s: []u8 = "abc";
    var one: ArrayList(SpanPair) = split_collect(a, s, 44);  // no ',' -> 1 field
    expect(one.len() == 1);
    var j1: []u8 = join(a, s, one, ",,,");          // no separator emitted
    expect(str_eq(j1, "abc"));
    free(a, j1);
    one.deinit(a);
}

test "property: join(split_collect(s, b), b) round-trips s" {
    var a: Allocator = c_allocator();
    // Byte-split fields can never contain the separator byte, so the
    // round-trip law holds for every input, including this gnarly one.
    var s1: []u8 = ",a,,bb,";
    var p1: ArrayList(SpanPair) = split_collect(a, s1, 44);
    expect(p1.len() == 5);                          // 4 seps + 1
    var j1: []u8 = join(a, s1, p1, ",");
    expect(str_eq(j1, s1));
    free(a, j1);
    p1.deinit(a);

    var s2: []u8 = "";
    var p2: ArrayList(SpanPair) = split_collect(a, s2, 44);
    var j2: []u8 = join(a, s2, p2, ",");
    expect(str_eq(j2, s2));
    free(a, j2);
    p2.deinit(a);

    var s3: []u8 = "no separator here";
    var p3: ArrayList(SpanPair) = split_collect(a, s3, 44);
    var j3: []u8 = join(a, s3, p3, ",");
    expect(str_eq(j3, s3));
    free(a, j3);
    p3.deinit(a);
}

test "property: StrSplitter spans + join round-trip a multi-byte sep" {
    var a: Allocator = c_allocator();
    var s: []u8 = "k1=v1;;k2=v2;;";
    var parts: ArrayList(SpanPair) = ArrayList(SpanPair).init(a);
    var it: StrSplitter = split_init_str(s, ";;");
    while (it.next()) {
        parts.push(a, SpanPair{ .off = it.cur_off, .len = it.cur_len });
    }
    expect(parts.len() == 3);                       // "k1=v1", "k2=v2", ""
    var j: []u8 = join(a, s, parts, ";;");
    expect(str_eq(j, s));
    free(a, j);
    parts.deinit(a);
}

test "replace: growth, shrink, delete" {
    var a: Allocator = c_allocator();
    var r1: []u8 = replace(a, "a-b-c", "-", "--");  // grow
    expect(str_eq(r1, "a--b--c"));
    free(a, r1);

    var r2: []u8 = replace(a, "abc", "b", "XYZ");   // grow in the middle
    expect(str_eq(r2, "aXYZc"));
    free(a, r2);

    var r3: []u8 = replace(a, "aXYZc", "XYZ", "b"); // shrink
    expect(str_eq(r3, "abc"));
    free(a, r3);

    var r4: []u8 = replace(a, "a,b,c", ",", "");    // delete all occurrences
    expect(str_eq(r4, "abc"));
    free(a, r4);

    var r5: []u8 = replace(a, "abc", "abc", "");    // whole string deleted
    expect(str_eq(r5, ""));
    expect(r5.len == 0);
    free(a, r5);
}

test "replace: boundaries and guards" {
    var a: Allocator = c_allocator();
    var r1: []u8 = replace(a, "abc", "z", "q");     // from absent: copy
    expect(str_eq(r1, "abc"));
    free(a, r1);

    var r2: []u8 = replace(a, "abc", "", "-");      // empty-from guard: copy
    expect(str_eq(r2, "abc"));
    free(a, r2);

    var r3: []u8 = replace(a, "", "a", "b");        // empty source
    expect(str_eq(r3, ""));
    free(a, r3);

    var r4: []u8 = replace(a, "ab", "abc", "x");    // from longer than s
    expect(str_eq(r4, "ab"));
    free(a, r4);

    var r5: []u8 = replace(a, "aba", "a", "z");     // match at both ends
    expect(str_eq(r5, "zbz"));
    free(a, r5);
}

test "replace: non-overlapping, resumes past the replacement" {
    var a: Allocator = c_allocator();
    var r1: []u8 = replace(a, "aaa", "aa", "b");    // match at 0 only
    expect(str_eq(r1, "ba"));
    free(a, r1);

    var r2: []u8 = replace(a, "aaaa", "aa", "b");   // matches at 0 and 2
    expect(str_eq(r2, "bb"));
    free(a, r2);

    var r3: []u8 = replace(a, "ab", "b", "bb");     // `to` contains `from`:
    expect(str_eq(r3, "abb"));                      // no rescan of the output
    free(a, r3);

    // "banana": "an" at 1 and 3 -> "bXXa"; length 6 + 2*(1-2) = 4.
    var r4: []u8 = replace(a, "banana", "an", "X");
    expect(str_eq(r4, "bXXa"));
    expect(r4.len == 4);
    free(a, r4);
}

test "property: replace(s, from, from) is the identity" {
    var a: Allocator = c_allocator();
    var r1: []u8 = replace(a, "the cat sat", "at", "at");
    expect(str_eq(r1, "the cat sat"));
    free(a, r1);
    var r2: []u8 = replace(a, "aaaa", "aa", "aa");
    expect(str_eq(r2, "aaaa"));
    free(a, r2);
}

test "pipeline: trim, split, replace each field, join" {
    var a: Allocator = c_allocator();
    var line: []u8 = trim("  red,green,blue \n");
    expect(str_eq(line, "red,green,blue"));
    var parts: ArrayList(SpanPair) = split_collect(a, line, 44);
    expect(parts.len() == 3);
    var j: []u8 = join(a, line, parts, "; ");
    expect(str_eq(j, "red; green; blue"));
    var r: []u8 = replace(a, j, "; ", "/");
    expect(str_eq(r, "red/green/blue"));
    free(a, r);
    free(a, j);
    parts.deinit(a);
}
