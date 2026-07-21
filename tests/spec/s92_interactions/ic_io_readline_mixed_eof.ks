//SPEC: §41 x §44 `@readLine` consumes fed stdin lines in order then yields empty at EOF; `@argc` is independent of stdin
//STDIN: alpha
//STDIN: beta
//OUT: alpha
//OUT: 4
//OUT: 0
//OUT: 1

pub fn main() void {
    var a: Allocator = c_allocator();
    var l1: []u8 = @readLine(a);
    print(l1);         // alpha
    var l2: []u8 = @readLine(a);
    print(l2.len);     // "beta" is 4 bytes
    var l3: []u8 = @readLine(a);
    print(l3.len);     // EOF → 0
    print(@argc());    // 1
    free(a, l1);
    free(a, l2);
    free(a, l3);
}
