//SPEC: §15.1 `&place` accepts an array index as the lvalue — writes through it land in the array
//OUT: 55

// Square every element strictly through element pointers, then sum by reading
// the array directly: 1 + 4 + 9 + 16 + 25 = 55.
pub fn main() void {
    var a: [5]i64 = [5]i64{ 1, 2, 3, 4, 5 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        var p: *i64 = &a[i];
        p.* = p.* * p.*;
    }

    var sum: i64 = 0;
    i = 0;
    while (i < 5) : (i += 1) {
        sum = sum + a[i];
    }
    print(sum);
}

// QUARANTINED (compiler bug, not a bad test): SPEC §15.1 lists an index as a
// valid `&place` lvalue and sema accepts it, but emit_c lowers the place via
// the bounds-checked rvalue getter — `(&(kd_arr_int64_t_5_get(kd_a, kd_i)))`
// — which is not a C lvalue, so cc fails ("lvalue required as unary '&'
// operand"). `&s[1]` on a slice fails identically via kd_slice_<tag>_get.
