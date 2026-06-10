//SPEC: §30.2 a pointer-receiver method on a SLICE element `s[i].m()` auto-refs the `_at` element pointer — the mutation lands in the backing storage
//OUT: 15
//OUT: 16

const C = struct {
    n: i64,
    fn bump(self: *C, by: i64) void {
        self.n += by;
    }
    fn get(self: C) i64 {
        return self.n;
    }
};

fn bump_all(s: []C, by: i64) void {
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        s[i].bump(by); // auto-ref via the bounds-checked slice `_at` pointer
    }
}

pub fn main() void {
    var arr: [2]C = [2]C{ C{ .n = 5 }, C{ .n = 6 } };
    bump_all(arr[0..2], 10);
    print(arr[0].get()); // 15 — mutation visible through the backing array
    print(arr[1].n);     // 16
}
