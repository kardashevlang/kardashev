//SPEC: §30.2 a pointer-receiver method called on an array element auto-refs the element IN PLACE (`a[i].m()` mutates `a[i]`, not a copy)
//OUT: 6
//OUT: 2
const C = struct {
    n: i64,
    fn inc(self: *C) void {
        self.n += 1;
    }
};

pub fn main() void {
    var arr: [2]C = [2]C{ C{ .n = 1 }, C{ .n = 5 } };
    arr[1].inc();
    print(arr[1].n);   // 6
    arr[0].inc();
    print(arr[0].n);   // 2
}
