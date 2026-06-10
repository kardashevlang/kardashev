//SPEC: §2 `a < b < c` parses (left-associative) but is a type error — `<` yields bool, and bool < int fails in sema
//ERR: E0110
pub fn main() void {
    if (1 < 2 < 3) {
        print(1);
    }
}
