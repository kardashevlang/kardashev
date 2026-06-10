//SPEC: §23.1 string byte indexing is bounds-checked — an out-of-range index panics (exit 101) after prior output
//EXIT: 101
//OUT: 3

fn bad_index() usize {
    return 9;   // computed at runtime so nothing folds the index away
}

pub fn main() void {
    var s: []u8 = "abc";
    print(s.len);
    print(s[bad_index()]);   // 9 >= 3: panic — this line never prints
}
