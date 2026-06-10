//SPEC: §15.2 slice bounds are runtime-checked — hi > len panics with exit 101
//EXIT: 101
//OUT: 2

fn hi_of(n: i64) i64 {
    return n * 2;
}

pub fn main() void {
    var data: [4]i64 = [4]i64{ 1, 2, 3, 4 };
    print(2);
    var hi: i64 = hi_of(3); // 6
    var s: []i64 = data[1..hi]; // 6 > 4: must panic
    print(s.len); // never reached
}
