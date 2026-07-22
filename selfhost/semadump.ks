// semadump.ks — driver for the self-hosted stage-27 sema mirror (v0.186).
//
//   kard run selfhost/semadump.ks -- <file.ks> [stdpath]
//
// Reads the file named by the first program argument, resolves it exactly
// like cdump (lexer.ks / parser.ks / modres.ks), and prints exactly ONE
// line to stdout:
//
//   ERROR <code> <pos>   the input fails to lex/parse/resolve (the cdump
//                        codes: 1/2 = E0001/E0002, 200/201 = E0200/E0201,
//                        291–294 structural; pos in concatenated
//                        coordinates);
//
//   SKIP <word> <pos>    the module parses but is outside the stage-27
//                        SEMA subset (`ss_detect`'s fixed walk — see
//                        selfhost/sema.ks; `import` covers every
//                        multi-file/std module: stage 27 is single-file);
//
//   OK                   the module is in the subset and the checker
//                        finds no diagnostic;
//
//   DIAG <code> <pos>    the module is in the subset and the FIRST
//                        diagnostic is E0<code> at byte <pos> — compared
//                        against the REAL Rust `sema::check`'s first
//                        diagnostic by `crates/kardc/tests/selfhost_sema.rs`.
//
// The exit code is always 0: every verdict is CAPTURED; the comparison is
// on stdout.

@import("lexer.ks");
@import("ast.ks");
@import("parser.ks");
@import("modres.ks");
// modres.ks decodes import-path string literals through the emitter's
// `es_decode_str` (the cdump flatten provides it the same way); §43 DCE
// keeps the rest of the emitter out of the binary.
@import("emit.ks");
@import("sema.ks");
@import("std");

/// Print one `<head> <num> <num>` line.
fn sd_line2(a: Allocator, head: []u8, x: i64, y: i64) void {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, head);
    sb.append_byte(a, 32);
    sb.append_i64(a, x);
    sb.append_byte(a, 32);
    sb.append_i64(a, y);
    var line: []u8 = sb.build(a);
    print(line);
    free(a, line);
    sb.deinit(a);
}

/// Print the `SKIP <word> <pos>` line.
fn sd_skip(a: Allocator, word: []u8, pos: usize) void {
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
    // The bundled std's source path (the cdump convention, v0.182): a std
    // import RESOLVES with it — and then stage 27's `import` skip fires
    // (multi-file modules are out of the sema subset), keeping the
    // resolver behaviour identical across the dump drivers.
    var stdp: []u8 = @arg(a, 2);

    var m: MrOut = mr_resolve(a, path, stdp);
    if (m.kind == MR_ERROR) {
        sd_line2(a, "ERROR", m.code, @as(i64, m.pos));
        return 0;
    }
    if (m.kind == MR_SKIP_STD) {
        sd_skip(a, "import", m.pos);
        return 0;
    }
    // Stage 27 is SINGLE-FILE: any resolved import (the flattener erases
    // the items, recording the first one's position) puts the module out
    // of the sema subset before the detector walk — mirroring the Rust
    // twin's import pre-pass.
    if (m.first_import >= 0) {
        sd_skip(a, "import", @as(usize, m.first_import));
        return 0;
    }

    var det: SsDet = ss_detect(m.src, m.nodes, m.root);
    if (det.found) {
        sd_skip(a, det.word, det.pos);
        return 0;
    }

    var v: SsOut = ss_verdict(a, m.src, m.nodes, m.root);
    if (v.code == 0) {
        print("OK");
        return 0;
    }
    sd_line2(a, "DIAG", v.code, @as(i64, v.pos));
    return 0;
}
