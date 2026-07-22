// modres.ks — self-host stage 9 (v0.167): the `modules::resolve` mirror.
//
// Resolves a root `.ks` file plus every file it reaches through
// `@import("…")` into ONE flat module over ONE merged arena, mirroring
// `crates/kardc/src/modules.rs` decision for decision:
//
//   - import paths resolve relative to the IMPORTING file's directory;
//   - the graph is walked depth-first, once per file (a diamond's shared
//     base is included once); a cycle is E0292; a missing/unreadable file
//     is E0291; a lex/parse error inside an IMPORTED file is E0294 (the
//     root's own lex/parse errors keep their structural codes 1/2/200/201);
//   - a file's imported items PRECEDE its own: pass 1 resolves the file's
//     imports in item order, pass 2 appends the file's own non-import
//     items;
//   - after the flatten, every top-level item name must be globally unique
//     (first collision → E0293 at the DUPLICATE item's position).
//
// Because the self-hosted AST stores names as SPANS (not decoded strings),
// the resolver builds a CONCATENATED virtual source: each file's bytes are
// appended in FIRST-READ (pre-order) order and its arena is merged with
// every span offset rebased by the file's base and every node link rebased
// by the arena base. All downstream positions (`ERROR`/`SKIP` lines, name
// slices, the interning scan) are in these concatenated coordinates; the
// Rust differential mirror assigns identical bases.
//
// `@import("std")` (a path whose basename is `std`/`std.ks` naming no
// real file) resolves to the bundled standard library (v0.182): the
// driver supplies the on-disk path of the SAME source the Rust compiler
// `include_str!`s, and std joins the flattened module like any import —
// dedup key `<std>`, silent on re-reach. Without a supplied path the
// pre-v0.182 `SKIP import` verdict stands at the import's position.
//
// Mirrored-with-documented-limits (both sides of the differential apply
// the SAME rule, so no comparison can diverge):
//   - dedup/cycle keys are LEXICALLY normalized paths (`.`/`..`/`//`
//     folding) where Rust canonicalizes — identical on the symlink-free
//     repo and temp trees the differential runs on;
//   - `@readFile` cannot distinguish a missing file from an empty one, so
//     an EMPTY import target reports E0291 (no corpus file imports an
//     empty file); an empty ROOT stays an empty module, as before.

@import("lexer.ks");
@import("ast.ks");
@import("parser.ks");
@import("std");

pub const MR_OK: i64 = 0;
pub const MR_ERROR: i64 = 1;
pub const MR_SKIP_STD: i64 = 2;

/// The resolver's outcome: `MR_OK` carries the merged module (`src`,
/// `nodes`, `root` = flattened item-chain head, -1 when empty); `MR_ERROR`
/// carries an `ERROR <code> <pos>` line's payload; `MR_SKIP_STD` the
/// position for the `SKIP import <pos>` line. Positions are concatenated
/// coordinates.
pub const MrOut = struct {
    kind: i64,
    code: i64,
    pos: usize,
    src: []u8,
    nodes: []Node,
    root: i32,
    // The first erased `@import` item's position (append order), -1 when
    // the module was single-file (v0.186 — see `Mr.first_import`).
    first_import: i64,
};

/// A registered file: its normalized path (a span into `pbuf`) and its
/// resolution state (1 = on the DFS stack, 2 = fully included).
pub const MrFile = struct {
    poff: usize,
    plen: usize,
    state: i64,
};

pub const Mr = struct {
    // The concatenated virtual source and the merged arena.
    src: []u8,
    src_len: usize,
    nodes: []Node,
    nd_len: usize,
    // The flattened item list (merged node indices, append order).
    flat: []i32,
    fl_len: usize,
    // The file registry (dedup + cycle detection) over a flat path buffer.
    pbuf: []u8,
    pb_len: usize,
    files: []MrFile,
    f_len: usize,
    // The failure latch (first error wins across the DFS).
    kind: i64,
    code: i64,
    pos: usize,
    // The bundled std's source path (v0.182): the selfhost pipeline has
    // no `include_str!`, so the DRIVER supplies where the compiler's
    // embedded `std.ks` lives on disk. Empty = unavailable, and a `std`
    // import keeps the pre-v0.182 `SKIP import` verdict.
    std_off: usize,
    std_len: usize,
    // The FIRST erased `@import` item's concatenated position in flat
    // append order, or -1 when the flattened module had none (v0.186):
    // the stage-27 sema differential is single-file, so its drivers
    // report `SKIP import <pos>` for any multi-file module — this is
    // that position, recorded as pass 2 skips the item.
    first_import: i64,

    fn init(a: Allocator) Self {
        return Mr{
            .src = alloc(a, u8, 4096),
            .src_len = 0,
            .nodes = alloc(a, Node, 256),
            .nd_len = 0,
            .flat = alloc(a, i32, 32),
            .fl_len = 0,
            .pbuf = alloc(a, u8, 256),
            .pb_len = 0,
            .files = alloc(a, MrFile, 8),
            .f_len = 0,
            .kind = MR_OK,
            .code = 0,
            .pos = 0,
            .std_off = 0,
            .std_len = 0,
            .first_import = 0 - 1,
        };
    }

    fn fail(self: *Self, code: i64, pos: usize) void {
        if (self.kind != MR_OK) { return; }
        self.kind = MR_ERROR;
        self.code = code;
        self.pos = pos;
    }

    fn skip_std(self: *Self, pos: usize) void {
        if (self.kind != MR_OK) { return; }
        self.kind = MR_SKIP_STD;
        self.pos = pos;
    }

    // -- growable stores ---------------------------------------------------------

    fn put_src(self: *Self, a: Allocator, s: []u8) void {
        var i: usize = 0;
        while (i < s.len) : (i += 1) {
            if (self.src_len == self.src.len) {
                var grown: []u8 = alloc(a, u8, self.src.len * 2);
                var j: usize = 0;
                while (j < self.src_len) : (j += 1) { grown[j] = self.src[j]; }
                free(a, self.src);
                self.src = grown;
            }
            self.src[self.src_len] = s[i];
            self.src_len += 1;
        }
    }

    fn put_node(self: *Self, a: Allocator, n: Node) void {
        if (self.nd_len == self.nodes.len) {
            var grown: []Node = alloc(a, Node, self.nodes.len * 2);
            var j: usize = 0;
            while (j < self.nd_len) : (j += 1) { grown[j] = self.nodes[j]; }
            free(a, self.nodes);
            self.nodes = grown;
        }
        self.nodes[self.nd_len] = n;
        self.nd_len += 1;
    }

    fn put_flat(self: *Self, a: Allocator, idx: i32) void {
        if (self.fl_len == self.flat.len) {
            var grown: []i32 = alloc(a, i32, self.flat.len * 2);
            var j: usize = 0;
            while (j < self.fl_len) : (j += 1) { grown[j] = self.flat[j]; }
            free(a, self.flat);
            self.flat = grown;
        }
        self.flat[self.fl_len] = idx;
        self.fl_len += 1;
    }

    fn put_path(self: *Self, a: Allocator, p: []u8) usize {
        var start: usize = self.pb_len;
        var i: usize = 0;
        while (i < p.len) : (i += 1) {
            if (self.pb_len == self.pbuf.len) {
                var grown: []u8 = alloc(a, u8, self.pbuf.len * 2);
                var j: usize = 0;
                while (j < self.pb_len) : (j += 1) { grown[j] = self.pbuf[j]; }
                free(a, self.pbuf);
                self.pbuf = grown;
            }
            self.pbuf[self.pb_len] = p[i];
            self.pb_len += 1;
        }
        return start;
    }

    /// Register `norm` with `state`; returns the registry slot.
    fn add_file(self: *Self, a: Allocator, norm: []u8, state: i64) usize {
        var off: usize = self.put_path(a, norm);
        if (self.f_len == self.files.len) {
            var grown: []MrFile = alloc(a, MrFile, self.files.len * 2);
            var j: usize = 0;
            while (j < self.f_len) : (j += 1) { grown[j] = self.files[j]; }
            free(a, self.files);
            self.files = grown;
        }
        self.files[self.f_len] = MrFile{ .poff = off, .plen = norm.len, .state = state };
        self.f_len += 1;
        return self.f_len - 1;
    }

    /// The registry slot of `norm`, or -1.
    fn find_file(self: *Self, norm: []u8) i64 {
        var i: usize = 0;
        while (i < self.f_len) : (i += 1) {
            var ent: []u8 = self.pbuf[self.files[i].poff .. self.files[i].poff + self.files[i].plen];
            if (str_eq(ent, norm)) { return @as(i64, i); }
        }
        return 0 - 1;
    }
};

// -- path arithmetic ------------------------------------------------------------------

/// The directory prefix of `p`, INCLUDING the trailing `/` (empty when `p`
/// has no separator).
pub fn mr_dir_of(a: Allocator, p: []u8) []u8 {
    var last: i64 = 0 - 1;
    var i: usize = 0;
    while (i < p.len) : (i += 1) {
        if (p[i] == 47) { last = @as(i64, i); }
    }
    if (last < 0) { return ""; }
    return p[0 .. @as(usize, last) + 1];
}

/// The final path segment of `p` (after the last `/`).
pub fn mr_basename(p: []u8) []u8 {
    var last: i64 = 0 - 1;
    var i: usize = 0;
    while (i < p.len) : (i += 1) {
        if (p[i] == 47) { last = @as(i64, i); }
    }
    if (last < 0) { return p; }
    return p[@as(usize, last) + 1 .. p.len];
}

/// Lexically normalize `p`: fold `.` and `//`, resolve `..` against the
/// preceding segment (kept literally when there is none to pop, so a
/// relative path may still escape upward). Preserves a leading `/`.
pub fn mr_normalize(a: Allocator, p: []u8) []u8 {
    // Segment spans collected into parallel arrays (worst case: every
    // segment kept).
    var segs: usize = 1;
    var i: usize = 0;
    while (i < p.len) : (i += 1) {
        if (p[i] == 47) { segs += 1; }
    }
    var offs: []usize = alloc(a, usize, segs);
    var lens: []usize = alloc(a, usize, segs);
    var count: usize = 0;
    var start: usize = 0;
    i = 0;
    while (i <= p.len) : (i += 1) {
        var at_end: bool = i == p.len;
        var is_sep: bool = false;
        if (!at_end) { is_sep = p[i] == 47; }
        if (at_end or is_sep) {
            var seg: []u8 = p[start .. i];
            if (seg.len == 0 or str_eq(seg, ".")) {
                // fold empty (double slash / leading slash) and `.`
            } else if (str_eq(seg, "..")) {
                var poppable: bool = count > 0;
                if (poppable) {
                    var top: []u8 = p[offs[count - 1] .. offs[count - 1] + lens[count - 1]];
                    if (str_eq(top, "..")) { poppable = false; }
                }
                if (poppable) {
                    count -= 1;
                } else {
                    offs[count] = start;
                    lens[count] = seg.len;
                    count += 1;
                }
            } else {
                offs[count] = start;
                lens[count] = seg.len;
                count += 1;
            }
            start = i + 1;
        }
    }
    var sb: StrBuilder = StrBuilder.init(a);
    if (p.len > 0 and p[0] == 47) { sb.append(a, "/"); }
    var k: usize = 0;
    while (k < count) : (k += 1) {
        if (k > 0) { sb.append(a, "/"); }
        sb.append(a, p[offs[k] .. offs[k] + lens[k]]);
    }
    var out: []u8 = sb.build(a);
    sb.deinit(a);
    free(a, offs);
    free(a, lens);
    return out;
}

/// `dir` (with trailing `/` or empty) + `rel`, normalized.
fn mr_join(a: Allocator, dir: []u8, rel: []u8) []u8 {
    var sb: StrBuilder = StrBuilder.init(a);
    sb.append(a, dir);
    sb.append(a, rel);
    var joined: []u8 = sb.build(a);
    sb.deinit(a);
    var norm: []u8 = mr_normalize(a, joined);
    free(a, joined);
    return norm;
}

// -- the resolver ----------------------------------------------------------------------

/// Lex + parse one file's source. Outcomes through `mr`'s latch: a lex or
/// parse failure fails with the structural code at its own position for
/// the ROOT (`is_root`), or with `E0294` at position 0 for an imported
/// file (`Span::DUMMY`, mirroring `sub_file_error`). Returns the file's
/// item-chain head IN MERGED COORDINATES (-1 for an empty module or on
/// failure), after merging the arena and source.
fn mr_load(mr: *Mr, a: Allocator, fsrc: []u8, is_root: bool) i32 {
    var toks: ArrayList(Token) = ArrayList(Token).init(a);
    var lx: Lexer = Lexer.init(fsrc);
    while (true) {
        var t: Token = lx.next();
        if (t.kind == TK_ERROR) {
            if (is_root) {
                mr.fail(@as(i64, t.len), mr.src_len + t.off);
            } else {
                mr.fail(294, 0);
            }
            return 0 - 1;
        }
        toks.push(a, t);
        if (t.kind == TK_EOF) { break; }
    }
    var p: Parser = Parser.init(a, fsrc, toks.items[0..toks.count]);
    var items: i32 = p.parse_module(a) catch 0 - 1;
    if (p.failed) {
        if (is_root) {
            mr.fail(p.ecode, mr.src_len + p.epos);
        } else {
            mr.fail(294, 0);
        }
        return 0 - 1;
    }
    // Merge: rebase spans by the source base and links by the arena base.
    var sbase: usize = mr.src_len;
    var nbase: i32 = @as(i32, mr.nd_len);
    mr.put_src(a, fsrc);
    var i: usize = 0;
    while (i < p.count) : (i += 1) {
        var n: Node = p.nodes[i];
        n.off = n.off + sbase;
        n.xoff = n.xoff + sbase;
        n.yoff = n.yoff + sbase;
        n.zoff = n.zoff + sbase;
        if (n.a >= 0) { n.a = n.a + nbase; }
        if (n.b >= 0) { n.b = n.b + nbase; }
        if (n.c >= 0) { n.c = n.c + nbase; }
        if (n.next >= 0) { n.next = n.next + nbase; }
        mr.put_node(a, n);
    }
    if (items < 0) { return 0 - 1; }
    return items + nbase;
}

/// `modules::resolve_file` + `process_source`: load `norm` (already
/// normalized; `import_pos` anchors structural diagnostics, in merged
/// coordinates), resolve its imports depth-first, then append its own
/// non-import items to the flat list.
fn mr_resolve_file(mr: *Mr, a: Allocator, norm: []u8, import_pos: usize, is_root: bool) void {
    if (mr.kind != MR_OK) { return; }

    // `@import("std")` — a `std`/`std.ks` basename naming no readable file
    // — is the compiler-embedded library (v0.182): it JOINS the flattened
    // module, loaded from the driver-supplied bundled-source path (the
    // same bytes `include_str!` embeds). The dedup key is `<std>`, and a
    // std reached again — on the stack or done — stops SILENTLY (the Rust
    // arm never reports a std cycle). Without a supplied path the
    // pre-v0.182 `SKIP import` verdict stands. (The `.exists()` mirror:
    // a readable non-empty file of that name next to the importer wins
    // and resolves normally.)
    var base: []u8 = mr_basename(norm);
    var fsrc: []u8 = @readFile(a, norm);
    var key: []u8 = norm;
    var is_std: bool = false;
    if ((str_eq(base, "std") or str_eq(base, "std.ks")) and fsrc.len == 0) {
        var stdsrc: []u8 = "";
        if (mr.std_len > 0) {
            stdsrc = @readFile(a, mr.pbuf[mr.std_off .. mr.std_off + mr.std_len]);
        }
        if (stdsrc.len == 0) {
            mr.skip_std(import_pos);
            return;
        }
        is_std = true;
        key = "<std>";
        fsrc = stdsrc;
        if (mr.find_file(key) >= 0) { return; }
    }

    if (!is_std) {
        // Cycle check before the dedup, so a true cycle is never
        // swallowed as "already included".
        var slot: i64 = mr.find_file(norm);
        if (slot >= 0) {
            if (mr.files[@as(usize, slot)].state == 1) {
                mr.fail(292, import_pos);
            }
            return;
        }

        // A missing/unreadable (or, through `@readFile`, empty) import is
        // E0291. The ROOT keeps the pre-v0.167 behavior: an empty read
        // parses as the empty module.
        if (fsrc.len == 0 and !is_root) {
            mr.fail(291, import_pos);
            return;
        }
    }

    var me: usize = mr.add_file(a, key, 1);

    var items: i32 = mr_load(mr, a, fsrc, is_root);
    if (mr.kind != MR_OK) {
        mr.files[me].state = 2;
        return;
    }

    // Pass 1 — imports, depth-first, in item order.
    var dir: []u8 = mr_dir_of(a, key);
    var cur: i32 = items;
    while (cur >= 0) {
        if (mr.kind != MR_OK) { break; }
        var u: usize = @as(usize, cur);
        if (mr.nodes[u].kind == ND_IMPORT) {
            // The x span is the path string token (quotes included), in
            // merged coordinates already.
            var raw: []u8 = es_decode_str(a, mr.src, mr.nodes[u].xoff, mr.nodes[u].xlen);
            var target: []u8 = mr_join(a, dir, raw);
            mr_resolve_file(mr, a, target, mr.nodes[u].off, false);
        }
        cur = mr.nodes[u].next;
    }
    if (mr.kind != MR_OK) {
        mr.files[me].state = 2;
        return;
    }

    // Pass 2 — this file's own non-import items, in order. The first
    // ERASED import's position is recorded for the single-file sema
    // differential (v0.186) — append order, exactly the order the Rust
    // twin walks the flattened files.
    cur = items;
    while (cur >= 0) {
        var u2: usize = @as(usize, cur);
        if (mr.nodes[u2].kind != ND_IMPORT) {
            mr.put_flat(a, cur);
        } else {
            if (mr.first_import < 0) {
                mr.first_import = @as(i64, mr.nodes[u2].off);
            }
        }
        cur = mr.nodes[u2].next;
    }

    mr.files[me].state = 2;
}

/// Whether flattened item `n` carries a globally-significant name (tests
/// do not; imports are erased before this point).
fn mr_named(nodes: []Node, n: i32) bool {
    var k: u8 = nodes[@as(usize, n)].kind;
    return k == ND_FN or k == ND_CONST or k == ND_STRUCT or k == ND_ENUM or k == ND_UNION or k == ND_ERRSET;
}

/// Resolve `root_path` into one flattened module (see the module header).
/// `std_path` names the bundled std's on-disk source (v0.182); empty
/// keeps a `std` import at the pre-v0.182 `SKIP import` verdict.
pub fn mr_resolve(a: Allocator, root_path: []u8, std_path: []u8) MrOut {
    var mr: Mr = Mr.init(a);
    if (std_path.len > 0) {
        mr.std_off = mr.put_path(a, std_path);
        mr.std_len = std_path.len;
    }
    var norm: []u8 = mr_normalize(a, root_path);
    mr_resolve_file(&mr, a, norm, 0, true);

    if (mr.kind == MR_OK) {
        // `check_unique` (E0293): the first duplicate top-level name, at
        // the DUPLICATE item's position.
        var i: usize = 0;
        while (i < mr.fl_len and mr.kind == MR_OK) : (i += 1) {
            if (!mr_named(mr.nodes, mr.flat[i])) { continue; }
            var iu: usize = @as(usize, mr.flat[i]);
            var iname: []u8 = mr.src[mr.nodes[iu].xoff .. mr.nodes[iu].xoff + mr.nodes[iu].xlen];
            var j: usize = 0;
            while (j < i) : (j += 1) {
                if (!mr_named(mr.nodes, mr.flat[j])) { continue; }
                var ju: usize = @as(usize, mr.flat[j]);
                var jname: []u8 = mr.src[mr.nodes[ju].xoff .. mr.nodes[ju].xoff + mr.nodes[ju].xlen];
                if (str_eq(iname, jname)) {
                    mr.fail(293, mr.nodes[iu].off);
                    break;
                }
            }
        }
    }

    // Re-wire the flattened item chain across the merged arena.
    var head: i32 = 0 - 1;
    if (mr.kind == MR_OK) {
        var prev: i32 = 0 - 1;
        var k: usize = 0;
        while (k < mr.fl_len) : (k += 1) {
            var it: i32 = mr.flat[k];
            if (prev < 0) {
                head = it;
            } else {
                mr.nodes[@as(usize, prev)].next = it;
            }
            prev = it;
        }
        if (prev >= 0) {
            mr.nodes[@as(usize, prev)].next = 0 - 1;
        }
    }

    return MrOut{
        .kind = mr.kind,
        .code = mr.code,
        .pos = mr.pos,
        .src = mr.src[0 .. mr.src_len],
        .nodes = mr.nodes,
        .root = head,
        .first_import = mr.first_import,
    };
}
