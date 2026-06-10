//SPEC: §30 x §26 — ArrayList(i32)'s pointer-receiver methods mutate through BOTH auto-ref and an explicit *ArrayList(i32)
//OUT: 4
//OUT: 60
//OUT: 1

@import("std");

// `l.push(…)` auto-refs the var; `double_all(&l)` mutates the SAME list
// through an explicit pointer parameter via `set` — the final sum sees both.
fn double_all(p: *ArrayList(i32)) void {
    var i: usize = 0;
    while (i < p.len()) : (i += 1) {
        p.set(i, p.get(i) * 2);     // pointer receiver auto-derefs
    }
}

pub fn main() void {
    var a: Allocator = c_allocator();
    var l: ArrayList(i32) = ArrayList(i32).init(a);
    var i: i32 = 1;
    while (i <= 4) : (i += 1) {
        l.push(a, i * i);           // 1, 4, 9, 16 — push takes *Self (auto-ref)
    }
    print(@as(i64, l.len()));       // 4
    double_all(&l);                 // 2, 8, 18, 32
    var t: i32 = 0;
    var j: usize = 0;
    while (j < l.len()) : (j += 1) {
        t += l.get(j);
    }
    print(t);                       // 60
    if (l.get(0) == 2) {            // first element really changed in place
        print(1);
    } else {
        print(0);
    }
    l.deinit(a);
}
