//SPEC: §15.2 slice bounds are runtime-checked — lo > hi panics with exit 101
//EXIT: 101
//OUT: 1

// The bad lo arrives from a function call so the check must happen at
// runtime; the print before the slice proves execution reached it.
fn lo_of(n: i64) i64 {
    return n + 3;
}

pub fn main() void {
    var data: [4]i64 = [4]i64{ 1, 2, 3, 4 };
    print(1);
    var lo: i64 = lo_of(0); // 3
    var s: []i64 = data[lo..2]; // 3 > 2: must panic
    print(s.len); // never reached
}
