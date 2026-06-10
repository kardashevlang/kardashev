//SPEC: §16 `c_allocator()` returns a first-class `Allocator` value — storable, copyable, usable for alloc and free
//OUT: 42

pub fn main() void {
    var a: Allocator = c_allocator();
    var b: Allocator = a; // a plain value copy

    var s: []i64 = alloc(a, i64, 3);
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        s[i] = (@as(i64, i) + 1) * 7; // 7 14 21
    }
    print(s[0] + s[1] + s[2]);

    free(b, s); // the copy frees what the original allocated
}
