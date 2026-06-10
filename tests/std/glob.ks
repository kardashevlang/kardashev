// tests/std/glob.ks — std glob module: glob_match / glob_is_literal
// (v0.157 std wave 2).

@import("std");

test "literal patterns match exactly" {
    expect(glob_match("abc", "abc"));
    expect(!glob_match("abc", "abd"));
    expect(!glob_match("abc", "ab"));       // pattern longer than text
    expect(!glob_match("abc", "abcd"));     // text longer than pattern
    expect(!glob_match("ABC", "abc"));      // case-sensitive
    expect(glob_match("hello world", "hello world"));
    expect(glob_match("", ""));             // empty/empty
    expect(!glob_match("", "a"));           // empty pattern, nonempty text
    expect(!glob_match("a", ""));           // nonempty pattern, empty text
}

test "star at start, middle, end" {
    expect(glob_match("*", ""));            // lone star matches empty
    expect(glob_match("*", "abc"));
    expect(glob_match("a*", "a"));          // trailing star matches empty run
    expect(glob_match("a*", "abc"));
    expect(!glob_match("a*", "ba"));
    expect(glob_match("*a", "a"));          // leading star matches empty run
    expect(glob_match("*a", "ba"));
    expect(!glob_match("*a", "ab"));
    expect(!glob_match("*a", ""));
    expect(glob_match("a*b", "ab"));        // middle star, empty run
    expect(glob_match("a*b", "axxb"));
    expect(!glob_match("a*b", "axxc"));
    // star must re-expand after the pattern was fully consumed mid-text:
    // the run is "ba", found only by resumed backtracking.
    expect(glob_match("a*b", "abab"));
}

test "multiple and adjacent stars" {
    expect(glob_match("a*b*c", "abc"));     // both stars empty
    expect(glob_match("a*b*c", "aXbYc"));
    expect(!glob_match("a*b*c", "acb"));    // order matters
    expect(glob_match("**", ""));           // adjacent stars collapse
    expect(glob_match("**", "x"));
    expect(glob_match("a**b", "ab"));
    expect(glob_match("a**b", "aXYb"));
    expect(glob_match("*ab*", "ab"));
    expect(glob_match("*ab*", "xxabyy"));
    expect(!glob_match("*ab*", "axb"));
    expect(glob_match("a*ab", "aab"));      // star empty on the first try
    expect(glob_match("*aab", "aaab"));     // star eats exactly one 'a'
    expect(glob_match("*a*", "bab"));
}

test "? matches exactly one byte" {
    expect(glob_match("?", "a"));
    expect(!glob_match("?", ""));           // ? cannot match empty
    expect(!glob_match("?", "ab"));         // ? matches exactly one
    expect(glob_match("??", "ab"));
    expect(!glob_match("??", "a"));
    expect(!glob_match("??", "abc"));
    expect(glob_match("a?c", "abc"));
    expect(glob_match("a?c", "axc"));       // any byte value
    expect(!glob_match("a?c", "ac"));
    expect(glob_match("?*", "a"));          // at least one byte
    expect(glob_match("?*", "abc"));
    expect(!glob_match("?*", ""));
    expect(glob_match("*?", "a"));
    expect(!glob_match("*?", ""));
}

test "classes: members and ranges" {
    expect(glob_match("[abc]", "a"));
    expect(glob_match("[abc]", "c"));
    expect(!glob_match("[abc]", "d"));
    expect(!glob_match("[abc]", ""));       // class consumes one byte
    expect(!glob_match("[abc]", "ab"));     // exactly one byte
    expect(glob_match("[a-z]", "a"));       // range is inclusive at both ends
    expect(glob_match("[a-z]", "m"));
    expect(glob_match("[a-z]", "z"));
    expect(!glob_match("[a-z]", "A"));      // 65 < 97: outside the range
    expect(!glob_match("[a-z]", "{"));      // 123, one past 'z' = 122
    expect(glob_match("[a-cx]", "x"));      // range + member mix
    expect(glob_match("[a-cx]", "b"));
    expect(!glob_match("[a-cx]", "d"));
    expect(glob_match("x[0-9]y", "x5y"));
    expect(!glob_match("x[0-9]y", "xay"));
    expect(glob_match("[0-9][0-9]", "42"));
}

test "classes: negation, ]-first, literal dash" {
    expect(glob_match("[!abc]", "d"));
    expect(!glob_match("[!abc]", "a"));
    expect(!glob_match("[!abc]", ""));      // negated class still needs a byte
    expect(glob_match("[!a-z]", "A"));
    expect(!glob_match("[!a-z]", "m"));
    expect(glob_match("[]]", "]"));         // ] first => literal member
    expect(!glob_match("[]]", "a"));
    expect(glob_match("[!]]", "a"));        // negated literal-] class
    expect(!glob_match("[!]]", "]"));
    expect(glob_match("[a-]", "-"));        // trailing - is literal
    expect(glob_match("[a-]", "a"));
    expect(!glob_match("[a-]", "b"));
    expect(glob_match("[-a]", "-"));        // leading - is literal
    expect(glob_match("[*]", "*"));         // metas are literal inside a class
    expect(!glob_match("[*]", "a"));
    expect(glob_match("[?]", "?"));
}

test "unterminated class is a literal [" {
    expect(glob_match("[abc", "[abc"));     // no closing ]: all literal
    expect(!glob_match("[abc", "a"));
    expect(glob_match("[]", "[]"));         // [] is unterminated (] is a member)
    expect(!glob_match("[]", ""));
    expect(glob_match("[", "["));
}

test "backslash escapes" {
    expect(glob_match("\\*", "*"));         // \* is a literal star
    expect(!glob_match("\\*", "a"));
    expect(!glob_match("\\*", ""));
    expect(glob_match("\\?", "?"));
    expect(!glob_match("\\?", "x"));
    expect(glob_match("a\\*b", "a*b"));
    expect(!glob_match("a\\*b", "aXb"));
    expect(!glob_match("a\\*b", "ab"));     // the escaped star is not a wildcard
    expect(glob_match("\\[abc]", "[abc]")); // escaped [ kills the class
    expect(glob_match("\\\\", "\\"));       // \\ pattern matches one backslash
    expect(glob_match("a\\", "a\\"));       // trailing lone \ is a literal \
    expect(!glob_match("a\\", "a"));
    expect(glob_match("\\a", "a"));         // escaping an ordinary byte is fine
}

test "pathological star backtracking terminates fast" {
    var a: Allocator = c_allocator();
    var txt: []u8 = alloc(a, u8, 200);
    fill(u8, txt, 97);                      // 200 x 'a'
    // a*a*a*a*b needs a leading 'a', three more 'a's in order, then a final
    // 'b' — 200 a's have no 'b', so this is the classic O(n*m) worst case.
    expect(!glob_match("a*a*a*a*b", txt));
    txt[199] = 98;                          // ...aaab: now 199 a's + 'b'
    expect(glob_match("a*a*a*a*b", txt));
    free(a, txt);
    // small exact-count variants, hand-checked:
    expect(!glob_match("a*a*a*a*b", "aaab"));   // only 3 a's before the b
    expect(glob_match("a*a*a*a*b", "aaaab"));   // exactly 4 a's, all stars empty
}

test "glob_is_literal" {
    expect(glob_is_literal(""));
    expect(glob_is_literal("abc"));
    expect(glob_is_literal("hello world"));
    expect(glob_is_literal("]"));           // bare ] is literal
    expect(glob_is_literal("a-b!c"));       // - and ! only matter inside [ ]
    expect(!glob_is_literal("*"));
    expect(!glob_is_literal("a?b"));
    expect(!glob_is_literal("[abc]"));
    expect(!glob_is_literal("a\\b"));
    expect(!glob_is_literal("["));          // conservative: still flagged
}

test "property: literal patterns behave like str_eq" {
    expect(glob_match("kard", "kard") == str_eq("kard", "kard"));
    expect(glob_match("kard", "karb") == str_eq("kard", "karb"));
    expect(glob_match("kard", "kar") == str_eq("kard", "kar"));
    expect(glob_match("", "") == str_eq("", ""));
    expect(glob_match("]", "]") == str_eq("]", "]"));
    expect(glob_match("]", "x") == str_eq("]", "x"));
}

test "property: prefix pattern equals str_starts_with" {
    expect(glob_match("ab*", "abc") == str_starts_with("abc", "ab"));
    expect(glob_match("ab*", "ab") == str_starts_with("ab", "ab"));
    expect(glob_match("ab*", "a") == str_starts_with("a", "ab"));
    expect(glob_match("ab*", "xab") == str_starts_with("xab", "ab"));
    expect(glob_match("ab*", "") == str_starts_with("", "ab"));
    expect(glob_match("ab*", "abxyz"));     // sanity: the true case is true
}

test "property: suffix pattern equals str_ends_with" {
    expect(glob_match("*xy", "xy") == str_ends_with("xy", "xy"));
    expect(glob_match("*xy", "zxy") == str_ends_with("zxy", "xy"));
    expect(glob_match("*xy", "xyz") == str_ends_with("xyz", "xy"));
    expect(glob_match("*xy", "") == str_ends_with("", "xy"));
    expect(glob_match("*xy", "wxy"));       // sanity: the true case is true
}

test "property: contains pattern equals str_index_of >= 0" {
    expect(glob_match("*c*", "abc") == (str_index_of("abc", 99) >= 0));
    expect(glob_match("*c*", "cab") == (str_index_of("cab", 99) >= 0));
    expect(glob_match("*c*", "ab") == (str_index_of("ab", 99) >= 0));
    expect(glob_match("*c*", "") == (str_index_of("", 99) >= 0));
    expect(glob_match("*c*", "c"));         // sanity: the true case is true
}
