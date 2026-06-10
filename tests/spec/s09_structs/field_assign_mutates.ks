//SPEC: §9.4 `p.f = e` assigns one field of a `var` struct, leaving sibling fields untouched
//OUT: 15
//OUT: 5
//OUT: 30
//OUT: 5
const Acc = struct {
    total: i32,
    count: i32,
};

pub fn main() void {
    var a: Acc = Acc{ .total = 0, .count = 0 };
    var i: i32 = 1;
    while (i <= 5) : (i += 1) {
        a.total = a.total + i;     // reads back the previous write each pass
        a.count = a.count + 1;
    }
    print(a.total);    // 1+2+3+4+5 = 15
    print(a.count);    // 5
    a.total = a.total * 2;
    print(a.total);    // 30
    print(a.count);    // 5 — sibling field untouched by the assignment above
}
