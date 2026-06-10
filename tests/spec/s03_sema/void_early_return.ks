//SPEC: §3 `return;` exits a `void` function early
//OUT: 2
//OUT: 3
fn show_small(n: i64) void {
    if (n > 3) {
        return; // early exit — nothing printed for large n
    }
    print(n);
}
pub fn main() void {
    show_small(2);
    show_small(5);
    show_small(3);
}
