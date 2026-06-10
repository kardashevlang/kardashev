//SPEC: §41 the `@readLine` result is a FRESH heap `[]u8` — byte-indexable and mutable (unlike a literal's statics)
//STDIN: kards
//OUT: 107
//OUT: 115
//OUT: Kards

pub fn main() void {
    var a: Allocator = c_allocator();
    var line: []u8 = @readLine(a);
    print(line[0]);    // 'k' = 107 — real byte access into the buffer
    print(line[4]);    // 's' = 115
    line[0] = 75;      // 'K' — heap-backed, so writes are allowed
    print(line);
}
