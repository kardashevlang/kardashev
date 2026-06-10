//SPEC: §11.1 `.?` (force-unwrap) whose operand is not an optional (`?T`) is E0182
//ERR: E0182

pub fn main() void {
    var n: i64 = 5;
    print(n.?);   // n is a plain i64, not a ?i64
}
