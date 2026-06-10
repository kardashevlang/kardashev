//SPEC: §16 distinct `alloc` calls return disjoint storage — writes to one slice never affect another
//OUT: 10000
//OUT: 100

pub fn main() void {
    var a: Allocator = c_allocator();
    var x: []i64 = alloc(a, i64, 4);
    var y: []i64 = alloc(a, i64, 4);

    // Interleaved writes into both live allocations.
    var i: usize = 0;
    while (i < 4) : (i += 1) {
        x[i] = @as(i64, i) + 1;        // 1 2 3 4
        y[i] = (@as(i64, i) + 1) * 10; // 10 20 30 40
    }

    // Mutate x heavily after y is fully written; y must be untouched.
    i = 0;
    while (i < 4) : (i += 1) {
        x[i] = x[i] * 1000;
    }

    var sx: i64 = 0;
    var sy: i64 = 0;
    i = 0;
    while (i < 4) : (i += 1) {
        sx = sx + x[i];
        sy = sy + y[i];
    }
    print(sx); // 1000+2000+3000+4000
    print(sy); // 10+20+30+40, undisturbed

    free(a, x);
    free(a, y);
}
