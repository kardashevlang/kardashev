//SPEC: §3 `expect` is valid only inside a `test` block
//ERR: E0140
pub fn main() void {
    expect(1 < 2);
}
