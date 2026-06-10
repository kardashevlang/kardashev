//SPEC: §20.1 a variant payload may be a slice; the capture is a full []T view (len, index, re-slice)
//OUT: 9
//OUT: 17
//OUT: corpus

const Data = union(enum) {
    text: []u8,
    nums: []i64,
};

fn weight(d: Data) i64 {
    switch (d) {
        .text => |s| {
            return @as(i64, s.len);
        },
        .nums => |xs| {
            var s: i64 = 0;
            var i: usize = 0;
            while (i < xs.len) : (i = i + 1) {
                s = s + xs[i];
            }
            return s;
        },
    }
}

pub fn main() void {
    print(weight(Data{ .text = "kardashev" }));     // 9 bytes
    var a: [4]i64 = [4]i64{ 2, 3, 5, 7 };
    print(weight(Data{ .nums = a[0..4] }));         // 17

    // The captured slice supports re-slicing (§15 applies to the capture).
    var d: Data = Data{ .text = "spec corpus" };
    switch (d) {
        .text => |s| {
            print(s[5..s.len]);                     // "corpus"
        },
        else => {
            print(0);
        },
    }
}
