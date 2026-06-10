//SPEC: §15.2 a slice can be sliced again — offsets compose relative to the inner view
//OUT: 3
//OUT: 3
//OUT: 99

pub fn main() void {
    var data: [8]i64 = [8]i64{ 0, 1, 2, 3, 4, 5, 6, 7 };
    var w: []i64 = data[2..8]; // 2 3 4 5 6 7
    var v: []i64 = w[1..4];    // 3 4 5  == data[3..6]
    print(v.len);
    print(v[0]); // data[3]

    // A write through the doubly-sliced view lands at data[3 + 2] = data[5].
    v[2] = 99;
    print(data[5]);
}
