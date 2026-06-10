//SPEC: §2/§28.1 binary operators at one level associate LEFT — a-b-c, a/b/c and a<<b>>c all group from the left
//OUT: 3
//OUT: 10
//OUT: 4
//OUT: 32
pub fn main() void {
    var ten: i64 = 10;
    // (10 - 4) - 3 = 3.  Right grouping 10 - (4 - 3) = 9.
    print(ten - 4 - 3);
    // (100 / 5) / 2 = 10.  Right grouping 100 / (5 / 2) = 50.
    print(100 / 5 / 2);
    // (1 << 3) >> 1 = 4.  Right grouping 1 << (3 >> 1) = 2.
    print(1 << 3 >> 1);
    // (64 >> 2) << 1 = 32.  Right grouping 64 >> (2 << 1) = 4.
    print(64 >> 2 << 1);
}
