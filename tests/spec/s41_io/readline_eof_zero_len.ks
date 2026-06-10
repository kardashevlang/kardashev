//SPEC: §41 `@readLine` at EOF yields a zero-length slice (indistinguishable from empty by design — no ![]u8)
//STDIN: only
//OUT: 4
//OUT: 0
//OUT: 0

pub fn main() void {
    var a: Allocator = c_allocator();
    var l1: []u8 = @readLine(a);
    print(l1.len);             // 4 — the single real line
    var l2: []u8 = @readLine(a);
    print(l2.len);             // 0 — stdin is exhausted
    var l3: []u8 = @readLine(a);
    print(l3.len);             // 0 — and stays exhausted on later reads
}
