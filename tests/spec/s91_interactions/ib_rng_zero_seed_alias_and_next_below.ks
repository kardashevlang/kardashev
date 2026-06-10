//SPEC: std Rng — seed 0 maps to the documented constant 88172645463325252 (identical stream); next_below(0) consumes NO draw, next_below(1) consumes one and yields 0
//OUT: 0
//OUT: 0
//OUT: 0
//OUT: 3

@import("std");

pub fn main() void {
    var r0: Rng = Rng.init(0);
    var rc: Rng = Rng.init(88172645463325252);
    var same: i64 = 0;

    if (r0.next_u64() == rc.next_u64()) {
        same += 1;            // draw 1 matches
    }

    var z: u64 = r0.next_below(0);
    print(@as(i64, z));       // 0, and — per the contract — no draw consumed

    if (r0.next_u64() == rc.next_u64()) {
        same += 1;            // still aligned: next_below(0) was draw-free
    }

    // next_below(1) consumes one draw on each generator and yields 0.
    print(@as(i64, r0.next_below(1)));
    print(@as(i64, rc.next_below(1)));

    if (r0.next_u64() == rc.next_u64()) {
        same += 1;            // aligned again after the paired draws
    }
    print(same);              // 3
}
