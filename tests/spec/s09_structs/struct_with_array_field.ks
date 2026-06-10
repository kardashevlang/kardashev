//SPEC: §9+§14.1 an index assignment writes through a struct-field place (`b.data[i] = e`)
//OUT: 4
//OUT: 30
//OUT: 9
//OUT: 4
const Buf = struct {
    data: [4]i32,
    n: i32,
};

pub fn main() void {
    var b: Buf = Buf{ .data = [4]i32{ 0, 0, 0, 0 }, .n = 0 };
    var i: i32 = 0;
    while (i < 4) : (i += 1) {
        b.data[i] = (i + 1) * (i + 1);   // squares: 1 4 9 16
        b.n = b.n + 1;
    }
    print(b.n);                          // 4
    var sum: i32 = 0;
    var j: i32 = 0;
    while (j < 4) : (j += 1) {
        sum = sum + b.data[j];           // reads back through the field
    }
    print(sum);                          // 1+4+9+16 = 30
    print(b.data[2]);                    // 9
    print(b.data.len);                   // 4
}
