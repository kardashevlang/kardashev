//SPEC: §3 a binding declared in a block is out of scope after the block's `}`
//ERR: E0100
pub fn main() void {
    {
        var inner: i64 = 5;
        print(inner);
    }
    print(inner); // `inner` died at the closing brace
}
