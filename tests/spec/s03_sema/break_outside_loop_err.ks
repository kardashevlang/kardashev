//SPEC: §3 `break` is only valid inside a loop body
//ERR: E0120
pub fn main() void {
    if (true) {
        break;
    }
}
