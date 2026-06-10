//SPEC: §41 an EMPTY stdin line yields a zero-length slice — and reading continues past it
//STDIN: ab
//STDIN:
//STDIN: cd
//OUT: 2
//OUT: 0
//OUT: 2
//OUT: cd

pub fn main() void {
    var a: Allocator = c_allocator();
    print(@readLine(a).len);   // 2
    print(@readLine(a).len);   // 0 — the empty middle line
    var l3: []u8 = @readLine(a);
    print(l3.len);             // 2 — the empty line did not end the stream
    print(l3);
}
