//SPEC: §14.1 arrays are value types — assignment copies the whole array, so the two mutate independently
//OUT: 1
//OUT: 100
//OUT: 3
//OUT: 12
//OUT: 105

pub fn main() void {
    var a: [3]i64 = [3]i64{ 1, 2, 3 };
    var b: [3]i64 = a;     // a full copy, not a view
    b[0] = 100;            // must not write through to `a`...
    a[2] = 9;              // ...and vice versa
    print(a[0]);                      // 1   (b's write did not alias a)
    print(b[0]);                      // 100
    print(b[2]);                      // 3   (a's write did not alias b)
    print(a[0] + a[1] + a[2]);        // 1 + 2 + 9
    print(b[0] + b[1] + b[2]);        // 100 + 2 + 3
}
