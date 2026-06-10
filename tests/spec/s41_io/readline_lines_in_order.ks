//SPEC: §41 each `@readLine(a)` returns the next stdin line as a fresh `[]u8` WITHOUT its trailing newline
//STDIN: hello
//STDIN: worlds
//OUT: 5
//OUT: hello
//OUT: 6
//OUT: worlds

pub fn main() void {
    var a: Allocator = c_allocator();
    var l1: []u8 = @readLine(a);
    print(l1.len);   // 5 — "hello" minus the newline
    print(l1);
    var l2: []u8 = @readLine(a);
    print(l2.len);   // 6 — successive calls consume successive lines
    print(l2);
}
