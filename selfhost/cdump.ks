// cdump.ks — driver for the self-hosted subset C emitter (v0.161).
//
//   kard run selfhost/cdump.ks -- <file.ks>
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
//                          the v0.161 scalar subset; <word> names the FIRST
//                          unsupported construct found by `es_detect`'s
//                          fixed depth-first walk and <pos> is its byte
//                          offset (`nomain 0` for a module with no `fn
//                          main`);
//
//   <C source>             the module is in the subset: the full
//                          `EmitMode::Program` C lowering from
//                          `selfhost/emit.ks`, byte-identical to the Rust
//                          emitter's output for every sema-valid program.
//
// The format contract lives in `crates/kardc/tests/selfhost_emit.rs`, whose
// Rust reference must produce these exact bytes (the C by running the real
// `kardc` pipeline, the SKIP verdict by a hand-mirrored walk of the Rust
// AST). The exit code is always 0: errors and skips are CAPTURED; the
// comparison is on stdout.

@import("lexer.ks");
@import("ast.ks");
@import("parser.ks");
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
    var src: []u8 = @readFile(a, path);

    // Lex everything up front, mirroring astdump: a lex error is the whole
    // output.
    var toks: ArrayList(Token) = ArrayList(Token).init(a);
    var lx: Lexer = Lexer.init(src);
    while (true) {
        var t: Token = lx.next();
        if (t.kind == TK_ERROR) {
            cd_error(a, @as(i64, t.len), t.off);
            return 0;
        }
        toks.push(a, t);
        if (t.kind == TK_EOF) { break; }
    }

    var p: Parser = Parser.init(a, src, toks.items[0..toks.count]);
    var items: i32 = p.parse_module(a) catch 0 - 1;
    if (p.failed) {
        cd_error(a, p.ecode, p.epos);
        return 0;
    }

    // An empty module (`items < 0`) has no `fn main`, so `es_detect` reports
    // it as `nomain` like any other main-less module.
    var det: Det = es_detect(src, p.nodes, items);
    if (det.found) {
        cd_skip(a, det.word, det.pos);
        return 0;
    }

    var c: []u8 = es_emit_program(a, src, p.nodes, items);
    // The C text is newline-terminated per line; `print` appends one more,
    // so print everything except the final newline.
    if (c.len > 0) {
        print(c[0 .. c.len - 1]);
    }
    return 0;
}
