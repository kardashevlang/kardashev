//SPEC: §36.1 with a capture, the catch operand must still be an error union (`!T`) — a plain value is E0192
//ERR: E0192

pub fn main() void {
    var n: i64 = 3;
    var y: i64 = n catch |e| @as(i64, e);   // n is a plain i64
    print(y);
}
