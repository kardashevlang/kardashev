//SPEC: §44 `@arg` yields an EMPTY slice for a negative index and for an index at or past `@argc()`, and the empty result is freeable
//OUT: 0
//OUT: 0

pub fn main() void {
    var a: Allocator = c_allocator();
    var neg: []u8 = @arg(a, 0 - 7);
    print(neg.len);
    var big: []u8 = @arg(a, @argc() + 5);
    print(big.len);
    free(a, neg);
    free(a, big);
}
