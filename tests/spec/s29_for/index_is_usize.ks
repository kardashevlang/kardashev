//SPEC: §29.1 the `, 0..` index binds as `usize` — it indexes another array directly and joins usize arithmetic
//OUT: 432
//OUT: 3

pub fn main() void {
    var zs: [3]i64 = [3]i64{ 2, 3, 4 };
    var ws: [4]i64 = [4]i64{ 1, 10, 100, 1000 };
    var total: i64 = 0;
    for (zs, 0..) |v, i| {
        total += v * ws[i];     // indexing wants usize — `i` is one already
    }
    print(total);               // 2*1 + 3*10 + 4*100 = 432
    var isum: usize = 0;
    for (zs, 0..) |v, i| {
        isum += i;              // usize += usize
    }
    print(isum);                // 0 + 1 + 2 = 3
}
