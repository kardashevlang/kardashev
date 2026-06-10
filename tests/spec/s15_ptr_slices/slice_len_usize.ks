//SPEC: §15.2 `s.len` is the view length (`usize`) — hi - lo for every ranged slice
//OUT: 4
//OUT: 21

pub fn main() void {
    var data: [6]i64 = [6]i64{ 9, 9, 9, 9, 9, 9 };

    var w: []i64 = data[2..6];
    print(w.len); // 6 - 2

    // Sum the lengths of every prefix window data[0..k] for k = 0..6:
    // 0 + 1 + 2 + 3 + 4 + 5 + 6 = 21. usize arithmetic throughout.
    var total: usize = 0;
    var k: usize = 0;
    while (k <= 6) : (k += 1) {
        var p: []i64 = data[0..k];
        total = total + p.len;
    }
    print(total);
}
