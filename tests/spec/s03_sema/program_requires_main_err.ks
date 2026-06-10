//SPEC: §3 the driver requires a `fn main` to build a program
//ERR: E0150
fn helper(n: i64) i64 {
    return n * 2;
}
