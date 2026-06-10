//SPEC: §29.2 a slice iterable copies only the {ptr,len} view — element writes during the loop ARE seen by later iterations
//OUT: 206
//OUT: 10
//OUT: 1030

// Contrast with array_iter_snapshot.ks: the temp holds the VIEW, the data is
// shared. iter0: v=1, s[0]=10, s[1]=2+100=102. iter1: v=102 (the write was
// seen), s[1]=1020, s[2]=3+100=103. iter2: v=103, s[2]=1030.
// sum = 1 + 102 + 103 = 206.
pub fn main() void {
    var xs: [3]i64 = [3]i64{ 1, 2, 3 };
    var s: []i64 = xs[0..3];
    var sum: i64 = 0;
    for (s, 0..) |v, i| {
        s[i] = v * 10;
        if (i < 2) {
            s[i + 1] = s[i + 1] + 100;
        }
        sum += v;
    }
    print(sum);      // 206
    print(xs[0]);    // 10
    print(xs[2]);    // 1030
}
