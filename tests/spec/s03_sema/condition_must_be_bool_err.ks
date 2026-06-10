//SPEC: §3 `if`/`while` conditions must be `bool`
//ERR: E0110
pub fn main() void {
    var n: i64 = 1;
    if (n) {
        print(1);
    }
    while (n - 1) {
        print(2);
    }
}
