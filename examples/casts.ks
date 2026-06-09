// casts.ks — integer casts with `@as(T, e)` (v0.137).
//
// `@as(T, e)` casts an integer value `e` to integer type `T`, lowering to a C
// cast. It bridges the otherwise-strict integer types — e.g. turning a signed
// `i32` key into a `usize` index — which is exactly what a hash map needs.

// A tiny hash bucket index for an i32 key (the core of a HashMap, v0.138).
fn bucket(key: i32, cap: usize) usize {
    var k: i32 = key;
    if (k < 0) {
        k = 0 - k;
    }
    return @as(usize, k) % cap;
}

pub fn main() i32 {
    // Signed/unsigned/width conversions.
    var n: i32 = 300;
    var u: usize = @as(usize, n);
    print(u);                       // 300
    var w: i64 = @as(i64, n) * @as(i64, 1000000);
    print(w);                       // 300000000
    var truncated: i32 = @as(i32, u + 5);
    print(truncated);               // 305

    // A cast used directly as an array index.
    var table: [16]i32 = [16]i32{ 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0 };
    var keys: [4]i32 = [4]i32{ 7, 23, 0 - 9, 100 };
    for (keys) |k| {
        table[bucket(k, 16)] += 1;   // count keys landing in each bucket
    }
    print(table[bucket(7, 16)]);     // 2  (7 and 23 both hash to bucket 7)
    print(table[bucket(23, 16)]);    // 2  (23 % 16 == 7 — collides with 7)
    print(table[7]);                 // 2  (bucket 7 holds both)
    print(table[9]);                 // 1  (|-9| % 16 == 9)
    print(table[4]);                 // 1  (100 % 16 == 4)
    return 0;
}

test "integer casts" {
    expect(@as(usize, 42) == 42);
    expect(@as(i64, 5) * 3 == 15);
    expect(bucket(0 - 9, 16) == 9);
    expect(bucket(23, 16) == 7);
    var x: usize = 1000;
    expect(@as(i32, x) == 1000);
}
