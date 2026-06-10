// tests/std/json.ks — std json module: json_parse / json_emit, the Json
// arena accessors (root/kind_of/num_at/str_at/key_at/str_decode/arr_len/
// arr_get/obj_get/deinit) and the JSON_* kind constants.

@import("std");

test "parse scalars" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "null");
    expect(j.ok);
    expect(j.root() == 0);
    expect(j.kind_of(j.root()) == JSON_NULL);
    j.deinit(a);

    j = json_parse(a, "true");
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_TRUE);
    j.deinit(a);

    j = json_parse(a, "false");
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_FALSE);
    j.deinit(a);

    // all four JSON whitespace bytes around a value (CR has no kardashev
    // escape, so the input is built byte-wise)
    var b: StrBuilder = StrBuilder.init(a);
    b.append(a, " \t");
    b.append_byte(a, 13);                       // CR
    b.append(a, "\n 42 \t");
    b.append_byte(a, 13);
    b.append(a, "\n ");
    var wsdoc: []u8 = b.build(a);
    b.deinit(a);
    j = json_parse(a, wsdoc);
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_NUM);
    expect(j.num_at(j.root()) == 42.0);
    expect(str_eq(j.str_at(j.root()), "42"));   // raw number text, ws excluded
    j.deinit(a);
    free(a, wsdoc);

    j = json_parse(a, "\"hi\"");
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_STR);
    expect(str_eq(j.str_at(j.root()), "hi"));
    j.deinit(a);
}

test "numbers: exact short decimals and exponents" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "0");
    expect(j.ok);
    expect(j.num_at(j.root()) == 0.0);
    j.deinit(a);

    j = json_parse(a, "-0");
    expect(j.ok);
    expect(j.num_at(j.root()) == 0.0);          // IEEE: -0 == 0
    j.deinit(a);

    j = json_parse(a, "3.5");
    expect(j.ok);
    expect(j.num_at(j.root()) == 3.5);
    j.deinit(a);

    j = json_parse(a, "2.75");
    expect(j.ok);
    expect(j.num_at(j.root()) == 2.75);
    j.deinit(a);

    j = json_parse(a, "-4");
    expect(j.ok);
    expect(j.num_at(j.root()) == 0.0 - 4.0);
    j.deinit(a);

    j = json_parse(a, "1e2");
    expect(j.ok);
    expect(j.num_at(j.root()) == 100.0);
    j.deinit(a);

    j = json_parse(a, "1E2");
    expect(j.ok);
    expect(j.num_at(j.root()) == 100.0);        // capital exponent marker
    j.deinit(a);

    j = json_parse(a, "25e-2");
    expect(j.ok);
    expect(j.num_at(j.root()) == 0.25);
    j.deinit(a);

    j = json_parse(a, "12.5e+1");
    expect(j.ok);
    expect(j.num_at(j.root()) == 125.0);
    j.deinit(a);

    j = json_parse(a, "0.1");
    expect(j.ok);
    // one exact-integer division: correctly rounded, equals the literal
    expect(j.num_at(j.root()) == 0.1);
    j.deinit(a);

    j = json_parse(a, "123456789");
    expect(j.ok);
    expect(j.num_at(j.root()) == 123456789.0);
    j.deinit(a);
}

test "numbers: saturation at the f64 edges" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "1e400");
    expect(j.ok);
    var v: f64 = j.num_at(j.root());
    expect(v > 0.0);
    expect(v / 10.0 == v);                       // +inf is a fixpoint of /10
    j.deinit(a);

    j = json_parse(a, "-1e400");
    expect(j.ok);
    v = j.num_at(j.root());
    expect(v < 0.0);
    expect(v / 10.0 == v);                       // -inf likewise
    j.deinit(a);

    j = json_parse(a, "1e-400");
    expect(j.ok);
    expect(j.num_at(j.root()) == 0.0);           // underflows to zero
    j.deinit(a);

    j = json_parse(a, "0e99999");
    expect(j.ok);
    expect(j.num_at(j.root()) == 0.0);           // zero mantissa stays zero
    j.deinit(a);
}

test "strings: zero-copy raw spans" {
    var a: Allocator = c_allocator();
    var src: []u8 = "{\"k\":\"hello\"}";
    var j: Json = json_parse(a, src);
    expect(j.ok);
    var v: i32 = j.obj_get(j.root(), "k");
    expect(v >= 0);
    expect(j.kind_of(v) == JSON_STR);
    var sv: []u8 = j.str_at(v);
    expect(sv.len == 5);
    expect(str_eq(sv, "hello"));
    expect(str_eq(sv, src[6..11]));              // same bytes as the input span
    expect(str_eq(j.key_at(v), "k"));
    j.deinit(a);
}

test "strings: every escape decodes" {
    var a: Allocator = c_allocator();
    // content tokens: a \n b \t c \\ d \" e \/ f \b g \f h \r i A j
    var j: Json = json_parse(a, "\"a\\nb\\tc\\\\d\\\"e\\/f\\bg\\fh\\ri\\u0041j\"");
    expect(j.ok);
    var raw: []u8 = j.str_at(j.root());
    expect(raw.len == 32);                       // 10 singles + 8*2 escapes + 6 for A
    expect(str_eq(raw, "a\\nb\\tc\\\\d\\\"e\\/f\\bg\\fh\\ri\\u0041j"));
    var s: []u8 = j.str_decode(a, j.root());
    expect(s.len == 19);
    expect(s[0] == 97);    // a
    expect(s[1] == 10);    // \n
    expect(s[3] == 9);     // \t
    expect(s[5] == 92);    // backslash
    expect(s[7] == 34);    // quote
    expect(s[9] == 47);    // slash (from \/)
    expect(s[11] == 8);    // \b
    expect(s[13] == 12);   // \f
    expect(s[15] == 13);   // \r
    expect(s[17] == 63);   // A -> '?' placeholder
    expect(s[18] == 106);  // j
    free(a, s);

    var j2: Json = json_parse(a, "\"x\\nyz\"");
    expect(j2.ok);
    var s2: []u8 = j2.str_decode(a, j2.root());
    expect(str_eq(s2, "x\nyz"));
    free(a, s2);
    // str_decode of a non-string node is a fresh empty slice
    var s3: []u8 = j2.str_decode(a, 0 - 1);
    expect(s3.len == 0);
    free(a, s3);
    j2.deinit(a);
    j.deinit(a);
}

test "arrays" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "[]");
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_ARR);
    expect(j.arr_len(j.root()) == 0);
    expect(j.arr_get(j.root(), 0) == 0 - 1);
    j.deinit(a);

    j = json_parse(a, "[1,2,3]");
    expect(j.ok);
    expect(j.arr_len(j.root()) == 3);
    expect(j.num_at(j.arr_get(j.root(), 0)) == 1.0);
    expect(j.num_at(j.arr_get(j.root(), 1)) == 2.0);
    expect(j.num_at(j.arr_get(j.root(), 2)) == 3.0);
    expect(j.arr_get(j.root(), 3) == 0 - 1);     // one past the end
    j.deinit(a);

    j = json_parse(a, "[[1],[2,3]]");
    expect(j.ok);
    expect(j.arr_len(j.root()) == 2);
    var inner: i32 = j.arr_get(j.root(), 1);
    expect(j.kind_of(inner) == JSON_ARR);
    expect(j.arr_len(inner) == 2);
    expect(j.num_at(j.arr_get(inner, 1)) == 3.0);
    j.deinit(a);

    j = json_parse(a, "[true,null,\"x\",1.5,[],{}]");
    expect(j.ok);
    expect(j.arr_len(j.root()) == 6);
    expect(j.kind_of(j.arr_get(j.root(), 0)) == JSON_TRUE);
    expect(j.kind_of(j.arr_get(j.root(), 1)) == JSON_NULL);
    expect(j.kind_of(j.arr_get(j.root(), 2)) == JSON_STR);
    expect(j.kind_of(j.arr_get(j.root(), 3)) == JSON_NUM);
    expect(j.kind_of(j.arr_get(j.root(), 4)) == JSON_ARR);
    expect(j.kind_of(j.arr_get(j.root(), 5)) == JSON_OBJ);
    j.deinit(a);
}

test "array element property: [0..19] round-trips through f64" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    b.append_byte(a, 91);                        // '['
    var k: i64 = 0;
    while (k < 20) : (k += 1) {
        if (k > 0) {
            b.append_byte(a, 44);                // ','
        }
        b.append_i64(a, k);
    }
    b.append_byte(a, 93);                        // ']'
    var doc: []u8 = b.build(a);
    b.deinit(a);

    var j: Json = json_parse(a, doc);
    expect(j.ok);
    expect(j.arr_len(j.root()) == 20);
    var i: i64 = 0;
    while (i < 20) : (i += 1) {
        expect(j.num_at(j.arr_get(j.root(), @as(usize, i))) == @as(f64, i));
    }
    expect(j.arr_get(j.root(), 20) == 0 - 1);
    j.deinit(a);
    free(a, doc);
}

test "objects: hits, misses, duplicates, empty keys" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "{}");
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_OBJ);
    expect(j.arr_len(j.root()) == 0);
    expect(j.obj_get(j.root(), "a") == 0 - 1);
    j.deinit(a);

    j = json_parse(a, "{\"a\":1,\"b\":2}");
    expect(j.ok);
    expect(j.arr_len(j.root()) == 2);
    expect(j.num_at(j.obj_get(j.root(), "a")) == 1.0);
    expect(j.num_at(j.obj_get(j.root(), "b")) == 2.0);
    expect(j.obj_get(j.root(), "c") == 0 - 1);   // miss
    expect(j.obj_get(j.root(), "A") == 0 - 1);   // keys are case-sensitive
    expect(str_eq(j.key_at(j.obj_get(j.root(), "b")), "b"));
    j.deinit(a);

    // duplicate keys: the FIRST match wins; both members are stored
    j = json_parse(a, "{\"a\":1,\"a\":2}");
    expect(j.ok);
    expect(j.arr_len(j.root()) == 2);
    expect(j.num_at(j.obj_get(j.root(), "a")) == 1.0);
    expect(j.num_at(j.arr_get(j.root(), 1)) == 2.0);
    j.deinit(a);

    // the empty key is legal JSON
    j = json_parse(a, "{\"\":7}");
    expect(j.ok);
    expect(j.num_at(j.obj_get(j.root(), "")) == 7.0);
    j.deinit(a);

    // nested object navigation
    j = json_parse(a, "{\"o\":{\"x\":5}}");
    expect(j.ok);
    var o: i32 = j.obj_get(j.root(), "o");
    expect(j.kind_of(o) == JSON_OBJ);
    expect(j.num_at(j.obj_get(o, "x")) == 5.0);
    j.deinit(a);

    // obj_get on a non-object misses; arr_get/num_at/str_at miss politely too
    j = json_parse(a, "[1]");
    expect(j.ok);
    expect(j.obj_get(j.root(), "a") == 0 - 1);
    expect(j.arr_get(j.arr_get(j.root(), 0), 0) == 0 - 1);  // index into a number
    expect(j.num_at(0 - 1) == 0.0);
    expect(j.str_at(j.root()).len == 0);          // str_at of an array is empty
    expect(j.kind_of(0 - 1) == JSON_BAD);
    expect(j.kind_of(9999) == JSON_BAD);
    j.deinit(a);
}

test "emit round-trips minified documents byte-for-byte" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "null");
    var out: []u8 = json_emit(a, j);
    expect(str_eq(out, "null"));
    free(a, out);
    j.deinit(a);

    j = json_parse(a, "[1,2,3]");
    out = json_emit(a, j);
    expect(str_eq(out, "[1,2,3]"));
    free(a, out);
    j.deinit(a);

    // numbers keep their exact source spelling (raw-span emission)
    j = json_parse(a, "[1e2,0.50,-7,-3.25]");
    out = json_emit(a, j);
    expect(str_eq(out, "[1e2,0.50,-7,-3.25]"));
    free(a, out);
    j.deinit(a);

    // string escapes pass through verbatim
    j = json_parse(a, "{\"s\":\"a\\nb\",\"t\":\"\\u0041\"}");
    out = json_emit(a, j);
    expect(str_eq(out, "{\"s\":\"a\\nb\",\"t\":\"\\u0041\"}"));
    free(a, out);
    j.deinit(a);

    // whitespace is normalised away
    j = json_parse(a, " [ 1 ,\t2 ,\n3 ] ");
    out = json_emit(a, j);
    expect(str_eq(out, "[1,2,3]"));
    free(a, out);
    j.deinit(a);

    j = json_parse(a, "{ \"a\" : [ true , null ] , \"b\" : \"x\" }");
    out = json_emit(a, j);
    expect(str_eq(out, "{\"a\":[true,null],\"b\":\"x\"}"));
    free(a, out);
    j.deinit(a);

    // property: emit is idempotent (emit(parse(emit(parse(d)))) == emit(parse(d)))
    j = json_parse(a, "{\"m\":[{},[],0.125,\"q\"],\"n\":false}");
    var out1: []u8 = json_emit(a, j);
    var j2: Json = json_parse(a, out1);
    expect(j2.ok);
    var out2: []u8 = json_emit(a, j2);
    expect(str_eq(out1, out2));
    expect(j2.num_at(j2.arr_get(j2.obj_get(j2.root(), "m"), 2)) == 0.125);
    free(a, out1);
    free(a, out2);
    j2.deinit(a);
    j.deinit(a);
}

test "a ~30-line document with every type and escapes" {
    var a: Allocator = c_allocator();
    var b: StrBuilder = StrBuilder.init(a);
    b.append(a, "{\n");
    b.append(a, "  \"name\": \"kardashev\",\n");
    b.append(a, "  \"version\": 0.157,\n");
    b.append(a, "  \"tags\": [\n");
    b.append(a, "    \"systems\",\n");
    b.append(a, "    \"zig\\tstyle\"\n");
    b.append(a, "  ],\n");
    b.append(a, "  \"config\": {\n");
    b.append(a, "    \"opt\": true,\n");
    b.append(a, "    \"debug\": false,\n");
    b.append(a, "    \"threads\": 8,\n");
    b.append(a, "    \"ratio\": 2.5,\n");
    b.append(a, "    \"extra\": null\n");
    b.append(a, "  },\n");
    b.append(a, "  \"matrix\": [\n");
    b.append(a, "    [1, 2],\n");
    b.append(a, "    [3, 4]\n");
    b.append(a, "  ],\n");
    b.append(a, "  \"empty_arr\": [],\n");
    b.append(a, "  \"empty_obj\": {},\n");
    b.append(a, "  \"esc\": \"line1\\nline2\",\n");
    b.append(a, "  \"deep\": {\n");
    b.append(a, "    \"a\": {\n");
    b.append(a, "      \"b\": [\n");
    b.append(a, "        {\n");
    b.append(a, "          \"c\": -12.25\n");
    b.append(a, "        }\n");
    b.append(a, "      ]\n");
    b.append(a, "    }\n");
    b.append(a, "  }\n");
    b.append(a, "}\n");
    var doc: []u8 = b.build(a);
    b.deinit(a);

    var j: Json = json_parse(a, doc);
    expect(j.ok);
    var r: i32 = j.root();
    expect(j.kind_of(r) == JSON_OBJ);
    expect(j.arr_len(r) == 9);

    expect(str_eq(j.str_at(j.obj_get(r, "name")), "kardashev"));
    expect(j.num_at(j.obj_get(r, "version")) == 0.157);
    expect(str_eq(j.key_at(j.arr_get(r, 0)), "name"));

    var tags: i32 = j.obj_get(r, "tags");
    expect(j.kind_of(tags) == JSON_ARR);
    expect(j.arr_len(tags) == 2);
    expect(str_eq(j.str_at(j.arr_get(tags, 0)), "systems"));
    expect(str_eq(j.str_at(j.arr_get(tags, 1)), "zig\\tstyle"));  // raw span
    var tag1: []u8 = j.str_decode(a, j.arr_get(tags, 1));
    expect(str_eq(tag1, "zig\tstyle"));                           // decoded
    free(a, tag1);

    var cfg: i32 = j.obj_get(r, "config");
    expect(j.arr_len(cfg) == 5);
    expect(j.kind_of(j.obj_get(cfg, "opt")) == JSON_TRUE);
    expect(j.kind_of(j.obj_get(cfg, "debug")) == JSON_FALSE);
    expect(j.num_at(j.obj_get(cfg, "threads")) == 8.0);
    expect(j.num_at(j.obj_get(cfg, "ratio")) == 2.5);
    expect(j.kind_of(j.obj_get(cfg, "extra")) == JSON_NULL);

    var mat: i32 = j.obj_get(r, "matrix");
    expect(j.arr_len(mat) == 2);
    expect(j.arr_len(j.arr_get(mat, 0)) == 2);
    expect(j.num_at(j.arr_get(j.arr_get(mat, 1), 0)) == 3.0);
    expect(j.num_at(j.arr_get(j.arr_get(mat, 1), 1)) == 4.0);

    expect(j.arr_len(j.obj_get(r, "empty_arr")) == 0);
    expect(j.arr_len(j.obj_get(r, "empty_obj")) == 0);
    expect(j.obj_get(j.obj_get(r, "empty_obj"), "x") == 0 - 1);

    var esc: []u8 = j.str_decode(a, j.obj_get(r, "esc"));
    expect(str_eq(esc, "line1\nline2"));
    free(a, esc);

    var deep: i32 = j.obj_get(r, "deep");
    var c: i32 = j.obj_get(j.arr_get(j.obj_get(j.obj_get(deep, "a"), "b"), 0), "c");
    expect(j.num_at(c) == 0.0 - 12.25);

    var out: []u8 = json_emit(a, j);
    expect(str_eq(out, "{\"name\":\"kardashev\",\"version\":0.157,\"tags\":[\"systems\",\"zig\\tstyle\"],\"config\":{\"opt\":true,\"debug\":false,\"threads\":8,\"ratio\":2.5,\"extra\":null},\"matrix\":[[1,2],[3,4]],\"empty_arr\":[],\"empty_obj\":{},\"esc\":\"line1\\nline2\",\"deep\":{\"a\":{\"b\":[{\"c\":-12.25}]}}}"));
    free(a, out);
    j.deinit(a);
    free(a, doc);
}

test "malformed inputs report the first error position" {
    var a: Allocator = c_allocator();

    var j: Json = json_parse(a, "");
    expect(!j.ok);
    expect(j.err_pos == 0);
    expect(j.root() == 0 - 1);
    var out: []u8 = json_emit(a, j);                // failed parse emits empty
    expect(out.len == 0);
    free(a, out);
    j.deinit(a);

    j = json_parse(a, "tru");                       // truncated literal
    expect(!j.ok);
    expect(j.err_pos == 0);
    j.deinit(a);

    j = json_parse(a, "[1,2");                      // truncated array
    expect(!j.ok);
    expect(j.err_pos == 4);
    j.deinit(a);

    j = json_parse(a, "[1 2]");                     // missing comma
    expect(!j.ok);
    expect(j.err_pos == 3);
    j.deinit(a);

    j = json_parse(a, "{\"a\"1}");                  // missing colon
    expect(!j.ok);
    expect(j.err_pos == 4);
    j.deinit(a);

    j = json_parse(a, "{\"a\":}");                  // missing value
    expect(!j.ok);
    expect(j.err_pos == 5);
    j.deinit(a);

    j = json_parse(a, "[1,]");                      // trailing comma (array)
    expect(!j.ok);
    expect(j.err_pos == 3);
    j.deinit(a);

    j = json_parse(a, "{\"a\":1,}");                // trailing comma (object)
    expect(!j.ok);
    expect(j.err_pos == 7);
    j.deinit(a);

    j = json_parse(a, "1 2");                       // trailing garbage
    expect(!j.ok);
    expect(j.err_pos == 2);
    expect(j.root() == 0 - 1);                      // garbage voids the root
    j.deinit(a);

    j = json_parse(a, "\"ab\\x\"");                 // bad escape at the 'x'
    expect(!j.ok);
    expect(j.err_pos == 4);
    j.deinit(a);

    j = json_parse(a, "\"abc");                     // unterminated string
    expect(!j.ok);
    expect(j.err_pos == 4);
    j.deinit(a);

    j = json_parse(a, "\"a\\u12g4\"");              // bad \u hex digit
    expect(!j.ok);
    expect(j.err_pos == 6);
    j.deinit(a);

    j = json_parse(a, "\"a\nb\"");                  // raw control byte in string
    expect(!j.ok);
    expect(j.err_pos == 2);
    j.deinit(a);

    j = json_parse(a, "{");                         // truncated object
    expect(!j.ok);
    expect(j.err_pos == 1);
    j.deinit(a);

    j = json_parse(a, "[");                         // truncated array head
    expect(!j.ok);
    expect(j.err_pos == 1);
    j.deinit(a);

    j = json_parse(a, "[01]");                      // leading zero
    expect(!j.ok);
    expect(j.err_pos == 2);
    j.deinit(a);

    j = json_parse(a, "-x");                        // sign without digits
    expect(!j.ok);
    expect(j.err_pos == 1);
    j.deinit(a);

    j = json_parse(a, "0.");                        // dot without fraction
    expect(!j.ok);
    expect(j.err_pos == 2);
    j.deinit(a);

    j = json_parse(a, "1e+");                       // exponent without digits
    expect(!j.ok);
    expect(j.err_pos == 3);
    j.deinit(a);
}

test "nesting depth: 64 parses, 65 is rejected" {
    var a: Allocator = c_allocator();

    var b: StrBuilder = StrBuilder.init(a);
    var i: i64 = 0;
    while (i < 64) : (i += 1) {
        b.append_byte(a, 91);                       // '['
    }
    i = 0;
    while (i < 64) : (i += 1) {
        b.append_byte(a, 93);                       // ']'
    }
    var doc64: []u8 = b.build(a);
    b.deinit(a);
    var j: Json = json_parse(a, doc64);
    expect(j.ok);
    expect(j.kind_of(j.root()) == JSON_ARR);
    // walk to the innermost array: 63 hops from the root
    var cur: i32 = j.root();
    var hops: i64 = 0;
    while (j.arr_len(cur) > 0) {
        cur = j.arr_get(cur, 0);
        hops += 1;
    }
    expect(hops == 63);
    expect(j.kind_of(cur) == JSON_ARR);
    j.deinit(a);
    free(a, doc64);

    var b2: StrBuilder = StrBuilder.init(a);
    i = 0;
    while (i < 65) : (i += 1) {
        b2.append_byte(a, 91);
    }
    i = 0;
    while (i < 65) : (i += 1) {
        b2.append_byte(a, 93);
    }
    var doc65: []u8 = b2.build(a);
    b2.deinit(a);
    var j2: Json = json_parse(a, doc65);
    expect(!j2.ok);
    expect(j2.err_pos == 64);                       // the 65th '[' breaks the cap
    expect(j2.root() == 0 - 1);
    j2.deinit(a);
    free(a, doc65);
}
