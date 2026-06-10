//SPEC: §38×§9×std sort — f64 struct fields read out in i64-key sorted order; binary-exact float sums survive the reordering
//OUT: 1.5
//OUT: 0.125
//OUT: 0.25
//OUT: 2.75
//OUT: 4.625
//OUT: 1

@import("std");

const Sample = struct {
    key: i64,
    val: f64,
};

pub fn main() void {
    // All values are binary-exact (k/2^n), so the sum 4.625 is exact too.
    var samples: [4]Sample = [4]Sample{
        Sample{ .key = 30, .val = 0.25 },
        Sample{ .key = 10, .val = 1.5 },
        Sample{ .key = 40, .val = 2.75 },
        Sample{ .key = 20, .val = 0.125 },
    };

    // Sort the int keys, then emit each sample's f64 in key order.
    var keys: [4]i64 = [4]i64{ 0, 0, 0, 0 };
    for (samples, 0..) |s, i| {
        keys[i] = s.key;
    }
    sort(i64, keys[0..4]);

    var acc: f64 = 0.0;
    for (keys) |k| {
        for (samples) |s| {
            if (s.key == k) {
                print(s.val);       // 1.5, 0.125, 0.25, 2.75
                acc = acc + s.val;
            }
        }
    }
    print(acc);                     // 4.625
    if (is_sorted(i64, keys[0..4])) {
        print(1);
    }
}
