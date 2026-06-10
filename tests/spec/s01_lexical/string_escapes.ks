//SPEC: §1 string escapes \n \t \\ \" decode to the single bytes 10, 9, 92, 34
//OUT: 5
//OUT: 65
//OUT: 10
//OUT: 9
//OUT: 92
//OUT: 34
//OUT: a
//OUT: b
pub fn main() void {
    // "A" + newline + tab + backslash + double-quote: five bytes.
    var s: []u8 = "A\n\t\\\"";
    print(s.len);
    var i: usize = 0;
    while (i < s.len) : (i = i + 1) {
        print(s[i]);
    }
    // The decoded \n is a real newline byte: this prints two lines.
    print("a\nb");
}
