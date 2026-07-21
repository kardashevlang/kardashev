//SPEC: §42 x §38 the std generic `sort` instantiated at f64 orders a double slice in place
//OUT: 1.25
//OUT: 2
//OUT: 3.5

@import("std");

pub fn main() void {
    var a: Allocator = c_allocator();
    var xs: []f64 = alloc(a, f64, 3);
    xs[0] = 3.5;
    xs[1] = 1.25;
    xs[2] = 2.0;
    sort(f64, xs);
    var i: usize = 0;
    while (i < xs.len) : (i += 1) {
        print(xs[i]);
    }
    free(a, xs);
}
