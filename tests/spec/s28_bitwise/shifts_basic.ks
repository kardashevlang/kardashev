//SPEC: §28.2 `<<`/`>>` shift left/right; a shift by zero is the identity; the amount may be a runtime value
//OUT: 1024
//OUT: 125
//OUT: 1000
//OUT: 1000
//OUT: 32
//OUT: 4

pub fn main() void {
    var one: i64 = 1;
    print(one << 10);          // 1024
    var v: i64 = 1000;
    print(v >> 3);             // 1000 / 8 = 125
    print(v << 0);             // shift-by-zero: unchanged
    print(v >> 0);             // shift-by-zero: unchanged
    var n: i64 = 5;
    print(one << n);           // a runtime shift amount: 32
    print((one << 62) >> 60);  // up then down: 2^62 >> 60 = 4
}
