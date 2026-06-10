//SPEC: §29.2 the array iterable is evaluated ONCE into a temp (a value copy) — mutating the source array mid-loop is not seen
//OUT: 6
//OUT: 100

pub fn main() void {
    var xs: [3]i64 = [3]i64{ 1, 2, 3 };
    var sum: i64 = 0;
    for (xs) |v| {
        xs[1] = 100;   // overwrite elements the loop has not reached yet...
        xs[2] = 100;
        sum += v;      // ...the loop still sees the snapshot {1, 2, 3}
    }
    print(sum);        // 1 + 2 + 3 = 6, NOT 1 + 100 + 100
    print(xs[1]);      // 100 — the writes really happened on the source
}
