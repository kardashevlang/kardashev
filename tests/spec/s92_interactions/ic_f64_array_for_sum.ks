//SPEC: §38 x §29 f64 arrays: a for-loop sum accumulates doubles; exactly-representable halves print cleanly under %g
//OUT: 10.5
//OUT: 2.625

pub fn main() void {
    var xs: [4]f64 = [4]f64{ 1.5, 2.25, 3.5, 3.25 };
    var sum: f64 = 0.0;
    for (xs) |x| {
        sum = sum + x;
    }
    print(sum);         // 1.5 + 2.25 + 3.5 + 3.25 = 10.5
    print(sum / 4.0);   // 2.625, exact in binary
}
