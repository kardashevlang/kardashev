//SPEC: §9.4+§15.2 a field assignment through a slice index (`s[i].f = e`) writes the underlying element's field
//OUT: 30
// QUARANTINED (v0.155 corpus, wave A): same hole as the array variant —
// sema accepts `s[1].x = 30` but emit_c lowers it through the by-value
// `kd_slice_<tag>_get(...)` accessor, so cc fails with "lvalue required as
// left operand of assignment".
const P = struct {
    x: i32,
    y: i32,
};

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 1, .y = 2 }, P{ .x = 3, .y = 4 } };
    var s: []P = arr[0..2];
    s[1].x = 30;
    print(arr[1].x);   // 30 — a slice is a view over the array
}
