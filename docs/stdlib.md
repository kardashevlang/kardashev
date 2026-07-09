# The standard library

`@import("std");` resolves to a standard library **embedded in the compiler**
(`crates/kardc/src/std.ks`, ~3,000 lines of kardashev) — there is no file to
install or ship. Its public items become available by bare name, and
dead-function elimination keeps it **pay-as-you-go**: code you don't call
adds nothing to your program's generated C.

Conventions, stated honestly:

- Everything that allocates takes an explicit `Allocator` (get one with
  `c_allocator()`) and pairs with a `deinit`/`free`.
- Where an error type is inexpressible today, the convention is documented
  at the item: parsers return optionals (`?i64`), decoders return an empty
  slice on malformed input, I/O returns `false`/empty.
- Every public item carries a `///` doc comment. Render the full API
  reference with the toolchain itself:

```console
$ kard doc crates/kardc/src/std.ks
```

This page is the guided overview. Line references are as of v0.179.0.

## Containers

| Type | API |
|------|-----|
| `ArrayList(T)` — growable array (doubling) | `init(a)`, `push(a, x)`, `get(i)`, `set(i, x)`, `len()`, `deinit(a)` |
| `HashMap(V)` — **`i32`-keyed** open-addressing map (tombstones, grow-and-rehash) | `init(a)`, `with_cap(a, c)`, `put(a, key, val)`, `get(key, fallback)`, `has(key)`, `remove(key)`, `len()`, `deinit(a)` |
| `Deque(T)` — growable ring buffer | `init(a)`, `push_front(a, v)`, `push_back(a, v)`, `pop_front()`, `pop_back()`, `front()`, `back()`, `len()`, `is_empty()`, `deinit(a)` |
| `BitSet` — dynamic bit set over `[]u64` | `init(a, nbits)`, `set(i)`, `clear(i)`, `toggle(i)`, `has(i)`, `count()`, `union_with(o)`, `intersect_with(o)`, `difference_with(o)`, `clear_all()`, `is_empty()`, `capacity()`, `deinit(a)` |

```rust
@import("std");

pub fn main() i32 {
    var a: Allocator = c_allocator();

    var q: Deque(i64) = Deque(i64).init(a);
    defer q.deinit(a);
    q.push_back(a, 2);
    q.push_front(a, 1);
    print(q.pop_front());        // 1
    print(q.pop_back());         // 2

    var counts: HashMap(i64) = HashMap(i64).init(a);
    defer counts.deinit(a);
    counts.put(a, 7, 40);
    counts.put(a, 7, counts.get(7, 0) + 2);   // upsert: read with fallback
    print(counts.get(7, 0));     // 42
    print(counts.get(9, -1));    // -1 (missing key → the fallback)
    return 0;
}
```

## Algorithms over slices

Generic where it matters (`comptime T: type`), `i64`-specialised reductions
where inference wants a concrete type:

- `sort(T, xs)` (quicksort, insertion below 17, median-of-3), `is_sorted(T, xs)`
- `binary_search(T, xs, x) i64` (index or −1), `index_of_elem(T, xs, x) i64`,
  `contains(T, xs, x) bool`
- `reverse(T, xs)`, `fill(T, xs, v)`, `copy_into(T, dst, src)`
- `sum64(xs)`, `min_in(xs)`, `max_in(xs)` over `[]i64`
- `shuffle(T, r, xs)` with a `*Rng`

## Integer math (`i32` + `i64`)

`imin`/`imax`/`iabs` (i32), `imin64`/`imax64`/`iabs64`/`sign`/`clamp64`,
`gcd`/`lcm`, `ipow` (by squaring), `isqrt` (Newton, exact at i64 extremes),
`div_floor`/`mod_floor` (floor semantics, unlike C's truncation) — all
overflow-safe by construction with documented preconditions.

## Text

- **Building:** `StrBuilder` — `init(a)`, `append(a, s)`, `append_byte(a, b)`,
  `append_i64(a, v)`, `len()`, `build(a) []u8`, `deinit(a)`.
- **Parsing:** `parse_i64(s) ?i64`, `parse_u64(s) ?u64`, `parse_f64(s) ?f64`
  — overflow-safe, exact at the extremes, `null` on malformed input.
- **Formatting:** `fmt_i64(a, v)`, `fmt_u64(a, v)`, `fmt_u64_hex(a, v)`,
  `fmt_i64_pad(a, v, width, zero_pad)`, `fmt_f64(a, v, decimals)`
  (fixed-point, correctly rounded).
- **Predicates & search:** `str_eq`, `str_starts_with`, `str_ends_with`,
  `str_index_of(s, c)`, `str_last_index_of(s, c)`, `str_count(s, c)`,
  `eq_ignore_case`, `to_lower(a, s)`, `to_upper(a, s)`, `str_concat(a, x, y)`.

## String splitting & rewriting

Zero-copy where possible — splitters iterate spans of the original slice
with a two-call protocol (`next()` then `current()`):

```rust
@import("std");

pub fn main() i32 {
    var sp: Splitter = split_init("a,bb,ccc", 44);   // 44 == ','
    while (sp.next()) {
        print(sp.current().len);    // 1, then 2, then 3
    }
    return 0;
}
```

Also: `split_init_str(s, sep)` (string separator), `split_collect(a, s, sep)
ArrayList(SpanPair)`, `trim`/`trim_start`/`trim_end` (zero-copy),
`join(a, src, parts, sep)`, `replace(a, s, from, to)` (non-overlapping).

## Formats

- **JSON** (`json_parse(a, src) Json`, `json_emit(a, j) []u8`): an
  arena-style parser — nodes in one `[]JsonNode` linked by indices, zero-copy
  string/key/number spans into the source. Strict grammar (escape
  validation, no leading zeros, no trailing commas/garbage, depth cap 64).
  On error `ok == false` and `err_pos` is the offset of the first bad byte.
  Inspect with `root()`, `kind_of(idx)` (the `JSON_*` constants),
  `num_at(idx)`, `str_at(idx)` / `str_decode(a, idx)`, `key_at(idx)`,
  `arr_len(idx)`, `arr_get(idx, i)`, `obj_get(idx, key)`; serialize back
  (minified, lossless) with `json_emit`.
- **base64** (`b64_encode(a, data)`, `b64_decode(a, text)` — RFC 4648,
  strict; empty slice on malformed input) and **hex** (`hex_encode`,
  `hex_decode`), plus `b64_encoded_len`/`b64_decoded_len`.

```rust
@import("std");

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var j: Json = json_parse(a, "{\"xs\": [1, 2, 3]}");
    defer j.deinit(a);
    print(j.arr_len(j.obj_get(j.root(), "xs")));   // 3

    var out: []u8 = json_emit(a, j);
    defer free(a, out);
    print(out);                                    // {"xs":[1,2,3]}

    var enc: []u8 = b64_encode(a, "kard");
    defer free(a, enc);
    print(enc);                                    // a2FyZA==
    return 0;
}
```

## Hashes & checksums

`crc32(data) u32` (one-shot) and the streaming `Crc32` (`init()`,
`update(data)`, `final()`), `fnv1a32`/`fnv1a64`, `adler32`, `djb2` — all
wrap-safe.

## Glob matching

`glob_match(pattern, text) bool` — iterative star-backtracking `*`, `?`,
`[a-z]`/`[!…]` classes and `\` escapes (dialect documented at the item;
pathological backtracking verified fast) — and `glob_is_literal(pattern)`.

## Random numbers

Deterministic xorshift64\* — `Rng.init(seed)`, `next_u64()`,
`next_below(n)`, `next_i64_in(lo, hi)` — plus Fisher–Yates
`shuffle(T, r, xs)`. Deterministic by design: same seed, same sequence,
pinned by tests.

## Where the std goes next

The std grows in waves (see [the roadmap](../ROADMAP-RUST-ZIG.md)), each
wave pure in-language code with hand-pinned `test` suites in
[`tests/std/`](../tests/std/). Adding to it is a great first contribution —
see [CONTRIBUTING.md](../CONTRIBUTING.md#standard-library-changes).
