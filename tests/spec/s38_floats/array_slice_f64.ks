//SPEC: §38 `[N]f64` arrays and `[]f64` slices hold doubles — literals, `for`, slicing, element writes
//OUT: 4.5
//OUT: 4
//OUT: 14.5

pub fn main() void {
    var xs: [3]f64 = [3]f64{ 0.5, 1.5, 2.5 };
    var sum: f64 = 0.0;
    for (xs) |v| {
        sum = sum + v;
    }
    print(sum);              // 0.5 + 1.5 + 2.5 = 4.5

    var tail: []f64 = xs[1..3];
    print(tail[0] + tail[1]); // 1.5 + 2.5 = 4

    tail[1] = 12.5;          // a slice write lands in the backing array
    print(xs[0] + xs[1] + xs[2]); // 0.5 + 1.5 + 12.5 = 14.5
}
