// heap.ks — the Allocator interface + heap (v0.119).
//
// Zig's law: there is no global allocator. You obtain an `Allocator` and pass
// it explicitly. `alloc(a, T, n)` returns a heap `[]T`; `free(a, s)` releases
// it. (v0.119 panics on OOM; error-returning alloc is a later item.)

fn fillSquares(buf: []i32) void {
    var i: usize = 0;
    var v: i32 = 0;
    while (i < buf.len) : (i = i + 1) {
        buf[i] = v * v;
        v = v + 1;
    }
}

fn sumSlice(s: []i32) i32 {
    var total: i32 = 0;
    var i: usize = 0;
    while (i < s.len) : (i = i + 1) {
        total = total + s[i];
    }
    return total;
}

pub fn main() i32 {
    var a: Allocator = c_allocator();

    var squares: []i32 = alloc(a, i32, 6);   // heap [6]i32 view
    fillSquares(squares);                    // 0 1 4 9 16 25
    print(squares.len);                      // 6
    print(squares[3]);                       // 9
    print(sumSlice(squares));                // 55
    free(a, squares);

    return 0;
}

test "heap squares" {
    var a: Allocator = c_allocator();
    var s: []i32 = alloc(a, i32, 4);
    fillSquares(s);                          // 0 1 4 9
    expect(s.len == 4);
    expect(s[2] == 4);
    expect(sumSlice(s) == 14);
    free(a, s);
}
