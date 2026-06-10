//SPEC: §11.1 `orelse` whose left operand is not an optional (`?T`) is E0181
//ERR: E0181

pub fn main() void {
    var n: i64 = 5;
    var y: i64 = n orelse 3;   // n is a plain i64, not a ?i64
    print(y);
}
