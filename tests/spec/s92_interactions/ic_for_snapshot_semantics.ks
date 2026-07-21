//SPEC: §29.2 the for iterable is evaluated ONCE into a by-value temp: body writes to the source array are invisible to the running iteration
//OUT: 1
//OUT: 2
//OUT: 3
//OUT: 9

pub fn main() void {
    var xs: [3]i64 = [3]i64{ 1, 2, 3 };
    for (xs, 0..) |x, i| {
        xs[i] = 9;    // mutate the SOURCE mid-iteration
        print(x);     // the snapshot still yields 1, 2, 3
    }
    print(xs[0]);     // the writes did land in the source
}
