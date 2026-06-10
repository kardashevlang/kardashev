//SPEC: §16 no global allocator — an `Allocator` is passed explicitly as an ordinary function parameter
//OUT: 21
//OUT: 54

// The helper can only allocate because its caller handed it the allocator.
fn make_fibs(a: Allocator, n: usize) []i64 {
    var s: []i64 = alloc(a, i64, n);
    s[0] = 1;
    s[1] = 1;
    var i: usize = 2;
    while (i < n) : (i += 1) {
        s[i] = s[i - 1] + s[i - 2];
    }
    return s;
}

pub fn main() void {
    var al: Allocator = c_allocator();
    var fibs: []i64 = make_fibs(al, 8); // 1 1 2 3 5 8 13 21
    print(fibs[7]);

    var sum: i64 = 0;
    var i: usize = 0;
    while (i < fibs.len) : (i += 1) {
        sum = sum + fibs[i];
    }
    print(sum); // 1+1+2+3+5+8+13+21

    free(al, fibs);
}
