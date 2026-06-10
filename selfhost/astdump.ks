// astdump.ks — driver for the self-hosted parser (v0.160).
//
//   kard run selfhost/astdump.ks -- <file.ks>
//
// Reads the file named by the first program argument, lexes it with
// `selfhost/lexer.ks`, parses it with `selfhost/parser.ks`, and prints the
// canonical line-based AST dump — ONE NODE PER LINE, depth-first, two spaces
// of indentation per tree level:
//
//   <indent><KIND> <off> <len>[ <extras>]
//
// The format contract (kind names, extras order, child order) is defined in
// `crates/kardc/tests/selfhost_parser.rs`, whose Rust reference dumper must
// produce these exact bytes from `kardc::parser::parse`'s AST. Summary:
//
//   - `<off> <len>` is the node's source span;
//   - name-carrying nodes append the name text (sliced from the source by
//     span; a `@This()` type base prints `Self`, the one name the reference
//     parser synthesizes);
//   - INT prints its decoded value; BOOL prints 0/1; FLOAT/STR/IMPORT/TEST
//     print no text (their spans pin the source bytes already);
//   - flag-like extras print as fixed words (`opt`, `err`, `ptr`, `slice`,
//     `app`, `cont`), valued extras as `key=value` (`errset=S`, `arr=N`,
//     `arrp=n`, `cap=x`, `label=l`, `idx=i`, `r=lo..hi`, `=V`);
//   - `ASSIGN`/`PASSIGN` print the compound-op name or `none`; `BIN`/`UNARY`
//     print their operator name.
//
// For an input that fails to LEX or PARSE the whole dump is exactly one
// line:
//
//   ERROR <code> <pos>          code: 1/2 = E0001/E0002, 200/201 = E0200/E0201
//
// where pos = the byte offset of the first diagnostic. The exit code is
// always 0: the dump CAPTURES the error; the comparison is on stdout.

@import("lexer.ks");
@import("ast.ks");
@import("parser.ks");
@import("std");

// --- line output ---------------------------------------------------------------

/// Start a dump line: indentation, the kind name, and the `off len` span.
fn ad_head(a: Allocator, sb: *StrBuilder, depth: i64, name: []u8, off: usize, len: usize) void {
    var i: i64 = 0;
    while (i < depth) : (i += 1) {
        sb.append(a, "  ");
    }
    sb.append(a, name);
    sb.append_byte(a, 32);
    sb.append_i64(a, @as(i64, off));
    sb.append_byte(a, 32);
    sb.append_i64(a, @as(i64, len));
}

/// Append ` text` to the line.
fn ad_word(a: Allocator, sb: *StrBuilder, w: []u8) void {
    sb.append_byte(a, 32);
    sb.append(a, w);
}

/// Append ` <n>` to the line.
fn ad_num(a: Allocator, sb: *StrBuilder, n: i64) void {
    sb.append_byte(a, 32);
    sb.append_i64(a, n);
}

/// Print the assembled line and release the builder.
fn ad_flush(a: Allocator, sb: *StrBuilder) void {
    var line: []u8 = sb.build(a);
    print(line);
    free(a, line);
    sb.deinit(a);
}

/// The primary name text of node `n`: its `x` span, or `Self` for a type
/// base written `@This()` (no source bytes spell the synthesized name).
fn ad_xname(src: []u8, nodes: []Node, n: i32) []u8 {
    var u: usize = @as(usize, n);
    if ((nodes[u].flags & F_THIS) != 0) {
        return "Self";
    }
    return src[nodes[u].xoff..nodes[u].xoff + nodes[u].xlen];
}

// --- the walker ------------------------------------------------------------------

/// Dump the sibling chain starting at `n` (-1 = empty) at `depth`.
fn ad_list(a: Allocator, src: []u8, nodes: []Node, n: i32, depth: i64) void {
    var cur: i32 = n;
    while (cur >= 0) {
        ad_node(a, src, nodes, cur, depth);
        cur = nodes[@as(usize, cur)].next;
    }
}

/// Dump one node (and its subtree) at `depth`. One arm per ND_* kind; the
/// child order mirrors the Rust reference dumper arm for arm.
fn ad_node(a: Allocator, src: []u8, nodes: []Node, n: i32, depth: i64) void {
    var u: usize = @as(usize, n);
    var k: u8 = nodes[u].kind;
    var off: usize = nodes[u].off;
    var len: usize = nodes[u].len;
    var fl: i64 = nodes[u].flags;
    var sb: StrBuilder = StrBuilder.init(a);

    if (k == ND_FN) {
        ad_head(a, &sb, depth, "FN", off, len);
        if ((fl & F_PUB) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // params
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // return type
        ad_node(a, src, nodes, nodes[u].c, depth + 1);      // body
        return;
    }
    if (k == ND_PARAM) {
        ad_head(a, &sb, depth, "PARAM", off, len);
        if ((fl & F_COMPTIME) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // type
        return;
    }
    if (k == ND_CONST) {
        ad_head(a, &sb, depth, "CONST", off, len);
        if ((fl & F_PUB) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        if (nodes[u].a >= 0) {
            ad_node(a, src, nodes, nodes[u].a, depth + 1);  // annotation
        }
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // value
        return;
    }
    if (k == ND_TEST) {
        ad_head(a, &sb, depth, "TEST", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // body
        return;
    }
    if (k == ND_STRUCT) {
        ad_head(a, &sb, depth, "STRUCT", off, len);
        if ((fl & F_PUB) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // fields
        ad_list(a, src, nodes, nodes[u].b, depth + 1);      // methods
        return;
    }
    if (k == ND_SFIELD) {
        ad_head(a, &sb, depth, "SFIELD", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // type
        return;
    }
    if (k == ND_ENUM) {
        ad_head(a, &sb, depth, "ENUM", off, len);
        if ((fl & F_PUB) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // variants
        return;
    }
    if (k == ND_VARIANT) {
        ad_head(a, &sb, depth, "VARIANT", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        if ((fl & F_VAL) != 0) {
            sb.append_byte(a, 32);
            sb.append_byte(a, 61);                           // '='
            sb.append_i64(a, nodes[u].val);
        }
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_UNION) {
        ad_head(a, &sb, depth, "UNION", off, len);
        if ((fl & F_PUB) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // variants
        return;
    }
    if (k == ND_UVAR) {
        ad_head(a, &sb, depth, "UVAR", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // payload type
        return;
    }
    if (k == ND_IMPORT) {
        ad_head(a, &sb, depth, "IMPORT", off, len);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_ERRSET) {
        ad_head(a, &sb, depth, "ERRSET", off, len);
        if ((fl & F_PUB) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        var m: i32 = nodes[u].a;                             // members, inline
        while (m >= 0) {
            ad_word(a, &sb, ad_xname(src, nodes, m));
            m = nodes[@as(usize, m)].next;
        }
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_TYPE) {
        ad_head(a, &sb, depth, "TYPE", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        if ((fl & F_OPT) != 0) { ad_word(a, &sb, "opt"); }
        if ((fl & F_ERR) != 0) { ad_word(a, &sb, "err"); }
        if ((fl & F_ERRSET) != 0) {
            ad_word(a, &sb, "errset=");
            // append the set name with no separating space
            if ((fl & F_ESETTHIS) != 0) {
                sb.append(a, "Self");
            } else {
                sb.append(a, src[nodes[u].yoff..nodes[u].yoff + nodes[u].ylen]);
            }
        }
        if ((fl & F_PTR) != 0) { ad_word(a, &sb, "ptr"); }
        if ((fl & F_SLICE) != 0) { ad_word(a, &sb, "slice"); }
        if ((fl & F_ARRLIT) != 0) {
            ad_word(a, &sb, "arr=");
            sb.append_i64(a, nodes[u].val);
        }
        if ((fl & F_ARRPARAM) != 0) {
            ad_word(a, &sb, "arrp=");
            sb.append(a, src[nodes[u].yoff..nodes[u].yoff + nodes[u].ylen]);
        }
        if ((fl & F_APP) != 0) { ad_word(a, &sb, "app"); }
        ad_flush(a, &sb);
        if ((fl & F_APP) != 0) {
            ad_list(a, src, nodes, nodes[u].a, depth + 1);  // ctor args
        }
        return;
    }
    if (k == ND_BLOCK) {
        ad_head(a, &sb, depth, "BLOCK", off, len);
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // statements
        return;
    }
    if (k == ND_LET) {
        ad_head(a, &sb, depth, "LET", off, len);
        if ((fl & F_CONST) != 0) { ad_num(a, &sb, 1); } else { ad_num(a, &sb, 0); }
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        if (nodes[u].a >= 0) {
            ad_node(a, src, nodes, nodes[u].a, depth + 1);  // annotation
        }
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // initializer
        return;
    }
    if (k == ND_ASSIGN) {
        ad_head(a, &sb, depth, "ASSIGN", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        if (nodes[u].val < 0) {
            ad_word(a, &sb, "none");
        } else {
            ad_word(a, &sb, opc_name(nodes[u].val));
        }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // value
        return;
    }
    if (k == ND_PASSIGN) {
        ad_head(a, &sb, depth, "PASSIGN", off, len);
        if (nodes[u].val < 0) {
            ad_word(a, &sb, "none");
        } else {
            ad_word(a, &sb, opc_name(nodes[u].val));
        }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // place
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // value
        return;
    }
    if (k == ND_RETURN) {
        ad_head(a, &sb, depth, "RETURN", off, len);
        ad_flush(a, &sb);
        if (nodes[u].a >= 0) {
            ad_node(a, src, nodes, nodes[u].a, depth + 1);  // value
        }
        return;
    }
    if (k == ND_IF) {
        ad_head(a, &sb, depth, "IF", off, len);
        if ((fl & F_CAP) != 0) {
            ad_word(a, &sb, "cap=");
            sb.append(a, ad_xname(src, nodes, n));
        }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // condition
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // then
        if (nodes[u].c >= 0) {
            ad_node(a, src, nodes, nodes[u].c, depth + 1);  // else
        }
        return;
    }
    if (k == ND_WHILE) {
        ad_head(a, &sb, depth, "WHILE", off, len);
        if ((fl & F_LABEL) != 0) {
            ad_word(a, &sb, "label=");
            sb.append(a, ad_xname(src, nodes, n));
        }
        if (nodes[u].b >= 0) { ad_word(a, &sb, "cont"); }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // condition
        if (nodes[u].b >= 0) {
            ad_node(a, src, nodes, nodes[u].b, depth + 1);  // continue stmt
        }
        ad_node(a, src, nodes, nodes[u].c, depth + 1);      // body
        return;
    }
    if (k == ND_FOR) {
        ad_head(a, &sb, depth, "FOR", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        if ((fl & F_IDX) != 0) {
            ad_word(a, &sb, "idx=");
            sb.append(a, src[nodes[u].yoff..nodes[u].yoff + nodes[u].ylen]);
        }
        if ((fl & F_LABEL) != 0) {
            ad_word(a, &sb, "label=");
            sb.append(a, src[nodes[u].zoff..nodes[u].zoff + nodes[u].zlen]);
        }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // iterable
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // body
        return;
    }
    if (k == ND_BREAK or k == ND_CONTINUE) {
        if (k == ND_BREAK) {
            ad_head(a, &sb, depth, "BREAK", off, len);
        } else {
            ad_head(a, &sb, depth, "CONTINUE", off, len);
        }
        if ((fl & F_LABEL) != 0) {
            ad_word(a, &sb, "label=");
            sb.append(a, ad_xname(src, nodes, n));
        }
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_DEFER or k == ND_ERRDEFER) {
        if (k == ND_DEFER) {
            ad_head(a, &sb, depth, "DEFER", off, len);
        } else {
            ad_head(a, &sb, depth, "ERRDEFER", off, len);
        }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // deferred stmt
        return;
    }
    if (k == ND_SWITCH) {
        ad_head(a, &sb, depth, "SWITCH", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // scrutinee
        ad_list(a, src, nodes, nodes[u].b, depth + 1);      // arms
        if (nodes[u].c >= 0) {
            ad_node(a, src, nodes, nodes[u].c, depth + 1);  // default block
        }
        return;
    }
    if (k == ND_ARM) {
        ad_head(a, &sb, depth, "ARM", off, len);
        if ((fl & F_CAP) != 0) {
            ad_word(a, &sb, "cap=");
            sb.append(a, ad_xname(src, nodes, n));
        }
        var r: i32 = nodes[u].b;                             // ranges, inline
        while (r >= 0) {
            ad_word(a, &sb, "r=");
            sb.append_i64(a, nodes[@as(usize, r)].val);
            sb.append(a, "..");
            sb.append_i64(a, nodes[@as(usize, r)].val2);
            r = nodes[@as(usize, r)].next;
        }
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // value labels
        ad_node(a, src, nodes, nodes[u].c, depth + 1);      // body
        return;
    }
    if (k == ND_INT) {
        ad_head(a, &sb, depth, "INT", off, len);
        ad_num(a, &sb, nodes[u].val);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_FLOAT) {
        ad_head(a, &sb, depth, "FLOAT", off, len);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_BOOL) {
        ad_head(a, &sb, depth, "BOOL", off, len);
        ad_num(a, &sb, nodes[u].val);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_IDENT) {
        ad_head(a, &sb, depth, "IDENT", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_UNARY) {
        ad_head(a, &sb, depth, "UNARY", off, len);
        ad_word(a, &sb, uop_name(nodes[u].val));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // operand
        return;
    }
    if (k == ND_BIN) {
        ad_head(a, &sb, depth, "BIN", off, len);
        ad_word(a, &sb, opc_name(nodes[u].val));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // lhs
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // rhs
        return;
    }
    if (k == ND_CALL) {
        ad_head(a, &sb, depth, "CALL", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // arguments
        return;
    }
    if (k == ND_COMPTIME) {
        ad_head(a, &sb, depth, "COMPTIME", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // operand
        return;
    }
    if (k == ND_SLIT) {
        ad_head(a, &sb, depth, "SLIT", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // initializers
        return;
    }
    if (k == ND_FINIT) {
        ad_head(a, &sb, depth, "FINIT", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // value
        return;
    }
    if (k == ND_FIELD) {
        ad_head(a, &sb, depth, "FIELD", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // base
        return;
    }
    if (k == ND_STR) {
        ad_head(a, &sb, depth, "STR", off, len);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_UNREACHABLE) {
        ad_head(a, &sb, depth, "UNREACHABLE", off, len);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_BUILTIN) {
        ad_head(a, &sb, depth, "BUILTIN", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // arguments
        return;
    }
    if (k == ND_STRUCTTYPE) {
        ad_head(a, &sb, depth, "STRUCTTYPE", off, len);
        ad_flush(a, &sb);
        ad_list(a, src, nodes, nodes[u].a, depth + 1);      // fields
        ad_list(a, src, nodes, nodes[u].b, depth + 1);      // methods
        return;
    }
    if (k == ND_MCALL) {
        ad_head(a, &sb, depth, "MCALL", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // receiver
        ad_list(a, src, nodes, nodes[u].b, depth + 1);      // arguments
        return;
    }
    if (k == ND_NULL) {
        ad_head(a, &sb, depth, "NULL", off, len);
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_ORELSE) {
        ad_head(a, &sb, depth, "ORELSE", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // lhs
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // rhs
        return;
    }
    if (k == ND_UNWRAP) {
        ad_head(a, &sb, depth, "UNWRAP", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // operand
        return;
    }
    if (k == ND_ERRLIT) {
        ad_head(a, &sb, depth, "ERRLIT", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_ENUMLIT) {
        ad_head(a, &sb, depth, "ENUMLIT", off, len);
        ad_word(a, &sb, ad_xname(src, nodes, n));
        ad_flush(a, &sb);
        return;
    }
    if (k == ND_ALIT) {
        ad_head(a, &sb, depth, "ALIT", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // element type
        ad_list(a, src, nodes, nodes[u].b, depth + 1);      // elements
        return;
    }
    if (k == ND_INDEX) {
        ad_head(a, &sb, depth, "INDEX", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // base
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // index
        return;
    }
    if (k == ND_ADDROF) {
        ad_head(a, &sb, depth, "ADDROF", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // place
        return;
    }
    if (k == ND_DEREF) {
        ad_head(a, &sb, depth, "DEREF", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // operand
        return;
    }
    if (k == ND_SLICEX) {
        ad_head(a, &sb, depth, "SLICE", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // base
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // lo
        ad_node(a, src, nodes, nodes[u].c, depth + 1);      // hi
        return;
    }
    if (k == ND_TRY) {
        ad_head(a, &sb, depth, "TRY", off, len);
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // operand
        return;
    }
    if (k == ND_CATCH) {
        ad_head(a, &sb, depth, "CATCH", off, len);
        if ((fl & F_CAP) != 0) {
            ad_word(a, &sb, "cap=");
            sb.append(a, ad_xname(src, nodes, n));
        }
        ad_flush(a, &sb);
        ad_node(a, src, nodes, nodes[u].a, depth + 1);      // operand
        ad_node(a, src, nodes, nodes[u].b, depth + 1);      // default
        return;
    }
    // Unreachable for a well-formed arena (internal kinds ND_MEMBER /
    // ND_RANGE are rendered inline by their parents); print a marker so a
    // walker bug is visible in the differential rather than silent.
    ad_head(a, &sb, depth, "BADNODE", off, len);
    ad_flush(a, &sb);
}

/// Print the single `ERROR <code> <pos>` line.
fn ad_error(a: Allocator, code: i64, pos: usize) void {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, "ERROR");
    sb.append_byte(a, 32);
    sb.append_i64(a, code);
    sb.append_byte(a, 32);
    sb.append_i64(a, @as(i64, pos));
    ad_flush(a, &sb);
}

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var path: []u8 = @arg(a, 1);
    var src: []u8 = @readFile(a, path);

    // Lex the whole input up front (the parser's peek2/peek3 lookahead wants
    // a buffer; the simplest faithful equivalent of the Rust token Vec). A
    // lexical error becomes the same ERROR line the lexer differential pins.
    var toks: ArrayList(Token) = ArrayList(Token).init(a);
    var lx: Lexer = Lexer.init(src);
    var t: Token = lx.next();
    while (t.kind != TK_EOF and t.kind != TK_ERROR) {
        toks.push(a, t);
        t = lx.next();
    }
    if (t.kind == TK_ERROR) {
        // TK_ERROR encodes the code in `len` and the position in `off`.
        ad_error(a, @as(i64, t.len), t.off);
        return 0;
    }
    toks.push(a, t);                                // the trailing EOF token

    var p: Parser = Parser.init(a, src, toks.items[0..toks.count]);
    var items: i32 = p.parse_module(a) catch 0 - 1;
    if (p.failed) {
        ad_error(a, p.ecode, p.epos);
        return 0;
    }
    ad_list(a, src, p.nodes, items, 0);
    return 0;
}
