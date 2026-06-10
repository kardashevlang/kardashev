//SPEC: §23.1 the empty string `""` is a zero-length `[]u8`; printing it (or an empty sub-slice) writes only the newline
//OUT: 0
//OUT:
//OUT: 0
//OUT:

pub fn main() void {
    var e: []u8 = "";
    print(e.len);
    print(e);                  // zero bytes + '\n' → an empty line

    var s: []u8 = "abcd";
    var none: []u8 = s[2..2];  // lo == hi → empty view of a non-empty string
    print(none.len);
    print(none);
}
