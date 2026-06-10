//SPEC: §23.1 a string is an ordinary `[]u8` value — a literal passes directly as a fn argument and a fn returns one
//OUT: 5
//OUT: dash
//OUT: 4

fn byte_len(s: []u8) usize {
    return s.len;
}

fn middle(s: []u8, lo: usize, hi: usize) []u8 {
    return s[lo..hi];
}

pub fn main() void {
    print(byte_len("hello"));
    // "kardashev"[3..7] = bytes 3,4,5,6 = 'd' 'a' 's' 'h'.
    var mid: []u8 = middle("kardashev", 3, 7);
    print(mid);
    print(mid.len);
}
