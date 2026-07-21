//SPEC: §16 x §38 `alloc(a, f64, n)`: fill through `@as(f64, i)` integer casts, read back exact multiples
//OUT: 0
//OUT: 2.5
//OUT: 5

pub fn main() void {
    var a: Allocator = c_allocator();
    var xs: []f64 = alloc(a, f64, 3);
    var i: usize = 0;
    while (i < xs.len) : (i += 1) {
        xs[i] = @as(f64, i) * 2.5;
    }
    print(xs[0]);   // 0
    print(xs[1]);   // 2.5
    print(xs[2]);   // 5
    free(a, xs);
}
