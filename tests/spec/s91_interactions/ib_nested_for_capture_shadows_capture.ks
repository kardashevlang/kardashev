//SPEC: §29.1 an inner `for` capture shadows an OUTER for's same-named capture inside the inner body — the outer capture is intact again after the inner loop
//OUT: 10
//OUT: 20
//OUT: 100
//OUT: 10
//OUT: 20
//OUT: 200

pub fn main() void {
    var xs: [2]i64 = [2]i64{ 1, 2 };
    var ys: [2]i64 = [2]i64{ 10, 20 };
    for (xs) |x| {
        for (ys) |x| {
            print(x);          // the INNER element: 10, 20
        }
        print(x * 100);        // the OUTER element again: 100, then 200
    }
}
