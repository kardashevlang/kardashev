//SPEC: §12.1 `catch` whose left operand is not an error union (`!T`) is E0192
//ERR: E0192

pub fn main() void {
    var n: i64 = 3;
    var y: i64 = n catch 0;   // n is a plain i64, not a !i64
    print(y);
}
