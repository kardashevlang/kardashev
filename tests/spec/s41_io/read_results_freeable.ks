//SPEC: §41.2 a read result is allocator-backed — `free(a, slice)` releases it like any §16 allocation
//STDIN: first
//STDIN: second
//OUT: first
//OUT: second
//OUT: 7

pub fn main() void {
    var a: Allocator = c_allocator();
    var l1: []u8 = @readLine(a);
    print(l1);
    free(a, l1);              // the buffer came from `a` — freeing is legal
    var l2: []u8 = @readLine(a);
    print(l2);                // reading still works after the free
    free(a, l2);
    print(7);                 // and the program runs to completion
}
