// cdump.ks — driver for the self-hosted subset C emitter (v0.161–v0.166).
//
//   kard run selfhost/cdump.ks -- <file.ks> [test]
//
// Reads the file named by the first program argument, lexes it with
// `selfhost/lexer.ks`, parses it with `selfhost/parser.ks`, and then prints
// exactly ONE of three things to stdout:
//
//   ERROR <code> <pos>     the input fails to lex or parse (same line the
//                          astdump driver prints: code 1/2 = E0001/E0002,
//                          200/201 = E0200/E0201; pos = first diagnostic);
//
//   SKIP <word> <pos>      the module parses but uses a construct outside
//                          the subset; <word> names the FIRST unsupported
//                          construct found by `es_detect`'s fixed
//                          depth-first walk and <pos> is its byte offset
//                          (`nomain 0` — Program mode only — for a module
//                          with no `fn main`);
//
//   <C source>             the module is in the subset: the full C
//                          lowering from `selfhost/emit.ks`, byte-identical
//                          to the Rust emitter's output for every
//                          sema-valid program — `EmitMode::Program` by
//                          default, `EmitMode::Test` (the test harness)
//                          when the second argument is `test` (v0.166).
//
// The format contract lives in `crates/kardc/tests/selfhost_emit.rs`, whose
// Rust reference must produce these exact bytes (the C by running the real
// `kardc` pipeline, the SKIP verdict by a hand-mirrored walk of the Rust
// AST). The exit code is always 0: errors and skips are CAPTURED; the
// comparison is on stdout.

@import("lexer.ks");
@import("ast.ks");
@import("parser.ks");
@import("modres.ks");
@import("emit.ks");
@import("std");

/// Print the single `ERROR <code> <pos>` line.
fn cd_error(a: Allocator, code: i64, pos: usize) void {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, "ERROR ");
    sb.append_i64(a, code);
    sb.append_byte(a, 32);
    sb.append_i64(a, @as(i64, pos));
    var line: []u8 = sb.build(a);
    print(line);
    free(a, line);
    sb.deinit(a);
}

/// Print the single `SKIP <word> <pos>` line.
fn cd_skip(a: Allocator, word: []u8, pos: usize) void {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, "SKIP ");
    sb.append(a, word);
    sb.append_byte(a, 32);
    sb.append_i64(a, @as(i64, pos));
    var line: []u8 = sb.build(a);
    print(line);
    free(a, line);
    sb.deinit(a);
}

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var path: []u8 = @arg(a, 1);
    var mode: []u8 = @arg(a, 2);
    var testmode: bool = str_eq(mode, "test");

    // Resolve the root and its transitive imports into one flattened
    // module over a concatenated virtual source (v0.167): a root lex/parse
    // failure keeps its structural code; an imported file's is E0294; a
    // missing import E0291, a cycle E0292, a duplicate top-level name
    // E0293 — all positions in concatenated coordinates. A `std` import is
    // the SKIP verdict `import` (the embedded library is out of subset).
    var m: MrOut = mr_resolve(a, path);
    if (m.kind == MR_ERROR) {
        cd_error(a, m.code, m.pos);
        return 0;
    }
    if (m.kind == MR_SKIP_STD) {
        cd_skip(a, "import", m.pos);
        return 0;
    }

    // In Program mode a main-less flattened module is `nomain`; Test mode
    // drops that gate (an empty module is the trivial harness).
    var det: Det = es_detect_mode(m.src, m.nodes, m.root, !testmode);
    if (det.found) {
        cd_skip(a, det.word, det.pos);
        return 0;
    }

    var c: []u8 = "";
    if (testmode) {
        c = es_emit_test(a, m.src, m.nodes, m.root);
    } else {
        c = es_emit_program(a, m.src, m.nodes, m.root);
    }
    // The C text is newline-terminated per line; `print` appends one more,
    // so print everything except the final newline.
    if (c.len > 0) {
        print(c[0 .. c.len - 1]);
    }
    return 0;
}
