//SPEC: §2 defer_stmt := "defer" stmt — defers a statement (including a block) to scope exit, running LIFO (§4.4)
//OUT: 1
//OUT: 2
//OUT: 3
//OUT: 5
//OUT: 6
//OUT: 7
pub fn main() void {
    print(1);
    {
        defer print(3); // flushes at the BLOCK's exit, not the function's
        print(2);
    }
    defer {
        print(7); // a block is a statement, so `defer { ... }` parses
    }
    defer print(6); // registered later -> runs first (LIFO)
    print(5);
}
