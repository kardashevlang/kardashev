//SPEC: §23.1 a string is a slice VALUE — a `var []u8` reassigns, and a copied binding keeps the old view
//OUT: 2
//OUT: 9
//OUT: kardashev
//OUT: 2
//OUT: hi

pub fn main() void {
    var s: []u8 = "hi";
    var keep: []u8 = s;   // copies the {ptr, len} header
    print(s.len);
    s = "kardashev";      // rebinding the var to another literal
    print(s.len);
    print(s);
    print(keep.len);      // the copy still views the old bytes
    print(keep);
}
