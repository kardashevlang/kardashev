//SPEC: §16 `alloc(a, u8, n)` builds a heap `[]u8` — printable and sliceable like any string (§23)
//OUT: ABCDE
//OUT: kBC

pub fn main() void {
    var a: Allocator = c_allocator();
    var bytes: []u8 = alloc(a, u8, 5);

    var i: usize = 0;
    while (i < bytes.len) : (i += 1) {
        bytes[i] = 65 + @as(u8, i); // 'A'..'E'
    }
    print(bytes);

    bytes[0] = 107; // 'k' — heap bytes are mutable, unlike a literal's statics
    var head: []u8 = bytes[0..3];
    print(head);

    free(a, bytes);
}
