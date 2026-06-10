//SPEC: §16 an alloc'd `[]T` is an ordinary slice — re-sliceable (§15.2) and passable to slice functions
//OUT: 3
//OUT: 12
//OUT: 30

fn sum_of(s: []i64) i64 {
    var t: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        t = t + s[i];
    }
    return t;
}

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: []i64 = alloc(a, i64, 6);
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        s[i] = @as(i64, i) + 1; // 1 2 3 4 5 6
    }

    var mid: []i64 = s[2..5]; // 3 4 5
    print(mid.len);
    print(sum_of(mid));

    mid[0] = 30; // the sub-view aliases the heap storage
    print(s[2]);

    free(a, s);
}
