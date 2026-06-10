//SPEC: §18.2 an inferred binding adopts an indexing expression's element type (and an array literal its array type)
//OUT: 2147483647
pub fn main() void {
    var a = [4]i32{ 10, 20, 30, 2147483587 }; // inferred: [4]i32
    var x = a[3];                  // inferred: i32
    var y = a[0] + a[1] + a[2];    // i32 arithmetic over elements = 60
    print(x + y);                  // 2147483587 + 60 = 2147483647 (i32 max)
}
