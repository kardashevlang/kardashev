// captures.ks — payload captures + `errdefer` (v0.125).
//
//   if (opt) |v| { ... } else { ... }   unwrap an optional, binding the value
//   errdefer stmt;                       run cleanup only on an error return

fn first_even(xs: []i32) ?i32 {
    var i: usize = 0;
    while (i < xs.len) : (i = i + 1) {
        if (xs[i] % 2 == 0) {
            return xs[i];
        }
    }
    return null;
}

// `errdefer` frees the scratch buffer only if this function returns an error;
// on success it is freed explicitly. (Optionals/error-unions wrap named types,
// so the function returns `!i32`, not `![]i32`.)
fn checked_sum(a: Allocator, fail: bool) !i32 {
    var buf: []i32 = alloc(a, i32, 3);
    errdefer free(a, buf);            // cleanup on the error path only
    if (fail) {
        return error.Rejected;        // errdefer fires → buf freed
    }
    buf[0] = 10;
    buf[1] = 20;
    buf[2] = 30;
    var sum: i32 = buf[0] + buf[1] + buf[2];
    free(a, buf);                     // success: free explicitly
    return sum;                       // 60
}

pub fn main() i32 {
    var data: [5]i32 = [5]i32{ 1, 3, 6, 9, 12 };
    if (first_even(data[0..5])) |e| {
        print(e);                     // 6
    } else {
        print(0 - 1);
    }
    if (first_even(data[0..2])) |e| {  // {1,3}: no even
        print(e);
    } else {
        print(0 - 1);                 // -1
    }

    var a: Allocator = c_allocator();
    print(checked_sum(a, false) catch 0 - 1);   // 60
    print(checked_sum(a, true) catch 0 - 1);     // -1 (errdefer freed the buffer)
    return 0;
}

test "captures + errdefer" {
    var xs: [4]i32 = [4]i32{ 1, 5, 8, 2 };
    if (first_even(xs[0..4])) |e| {
        expect(e == 8);
    } else {
        expect(false);
    }
    var a: Allocator = c_allocator();
    expect((checked_sum(a, false) catch 0) == 60);
    expect((checked_sum(a, true) catch 55) == 55);
}
