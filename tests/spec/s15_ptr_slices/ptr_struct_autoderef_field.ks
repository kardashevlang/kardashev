//SPEC: §30.1 `p.field` on a `*Struct` auto-derefs — reads and writes go through the pointer
//OUT: 15
//OUT: 5
//OUT: 75

const Acc = struct {
    total: i64,
    count: i64,
};

pub fn main() void {
    var acc: Acc = Acc{ .total = 0, .count = 0 };
    var p: *Acc = &acc;

    var k: i64 = 1;
    while (k <= 5) : (k += 1) {
        p.total = p.total + k; // write through the pointer
        p.count += 1;          // compound assign through the pointer
    }

    print(acc.total);          // 1+2+3+4+5, observed on the struct itself
    print(acc.count);
    print(p.total * p.count);  // reads through the pointer: 15 * 5
}
