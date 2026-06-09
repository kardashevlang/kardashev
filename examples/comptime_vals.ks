// comptime_vals.ks — comptime value parameters (v0.128).
//
// A `comptime n: usize` parameter is a compile-time value: the function is
// monomorphised per distinct `n`, and `n` can appear in array-size types
// (`[n]T`) and as a value in the body. Type arguments (v0.120) and value
// arguments compose.

// `n` sizes the parameter arrays AND bounds the loop; instantiated per length.
fn dot(comptime n: usize, a: [n]i32, b: [n]i32) i32 {
    var total: i32 = 0;
    var i: usize = 0;
    while (i < n) : (i = i + 1) {
        total = total + a[i] * b[i];
    }
    return total;
}

// A comptime value `reps` that is purely a compile-time count.
fn scaled_sum(comptime reps: i32, x: i32) i32 {
    var total: i32 = 0;
    var i: i32 = 0;
    while (i < reps) : (i = i + 1) {
        total = total + x;
    }
    return total;
}

pub fn main() i32 {
    print(dot(3, [3]i32{ 1, 2, 3 }, [3]i32{ 4, 5, 6 }));   // 32  (4 + 10 + 18)
    print(dot(2, [2]i32{ 10, 20 }, [2]i32{ 3, 4 }));        // 110 (30 + 80)
    print(scaled_sum(5, 7));                                // 35  (7 * 5)
    print(scaled_sum(3, 10));                               // 30
    return 0;
}

test "comptime values" {
    expect(dot(3, [3]i32{ 1, 1, 1 }, [3]i32{ 2, 2, 2 }) == 6);
    expect(dot(4, [4]i32{ 1, 0, 1, 0 }, [4]i32{ 9, 9, 9, 9 }) == 18);
    expect(scaled_sum(4, 25) == 100);
}
