//SPEC: §9.4+§14.1 a field assignment whose chain passes through an array index (`arr[i].f = e`) writes the element's field
//OUT: 30
//OUT: 4
// QUARANTINED (v0.155 corpus, wave A): sema accepts this place expression
// (no diagnostic), but emit_c lowers the read path through the by-value
// accessor `kd_arr_<tag>_<N>_get(...)`, producing `(get(arr,1)).kd_x = 30;`
// — not an lvalue, so the C compile fails. Either sema should reject the
// mixed index-then-field place with a diagnostic, or emit should lower it
// as a real lvalue write. The whole-element round-trip (copy out, mutate,
// store back) works and is pinned in tests/spec/s09_structs/struct_in_array.ks.
const P = struct {
    x: i32,
    y: i32,
};

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 1, .y = 2 }, P{ .x = 3, .y = 4 } };
    arr[1].x = 30;
    print(arr[1].x);   // 30
    print(arr[1].y);   // 4
}
