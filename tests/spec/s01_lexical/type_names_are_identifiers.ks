//SPEC: §1 type names (i32, usize, ...) are not keywords — they are ordinary identifiers and may name locals
//OUT: 15
//OUT: 8
pub fn main() void {
    var i32: i64 = 5;
    var usize: i64 = 3;
    print(i32 * usize);
    i32 = i32 + usize;
    print(i32);
}
