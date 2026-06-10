//SPEC: §3 `usize` is a distinct integer type: it never mixes with `u64` without a cast
//ERR: E0110
pub fn main() void {
    var n: usize = 1;
    var m: u64 = 2;
    print(n + m);
}
