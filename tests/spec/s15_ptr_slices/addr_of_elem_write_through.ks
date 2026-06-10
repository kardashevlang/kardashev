//SPEC: §15.1 writes through `&place` element pointers (`&arr[i]`, `&s[i]`, `&arr[i].f`) land in the backing array
//OUT: 77
//OUT: 9
//OUT: 5
//OUT: 42
const P = struct {
    x: i32,
};

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 1 }, P{ .x = 2 } };
    var p: *P = &arr[1];
    p.x = 77;                  // through a struct-element pointer (§30.1 auto-deref)
    print(arr[1].x);           // 77

    var a: [3]i64 = [3]i64{ 1, 2, 3 };
    var s: []i64 = a[0..3];
    var q: *i64 = &s[1];
    q.* = 9;                   // through a slice-element pointer
    print(a[1]);               // 9 — the slice views the array
    var r: *i64 = &a[2];
    r.* = r.* + 2;             // read AND write through the element pointer
    print(a[2]);               // 5

    var fx: *i32 = &arr[0].x;  // & of a field reached through an index
    fx.* = 42;
    print(arr[0].x);           // 42
}
