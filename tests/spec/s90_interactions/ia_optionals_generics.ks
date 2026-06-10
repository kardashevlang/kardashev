//SPEC: §11 x §17 — a generic fn may return ?T; each instantiation's optional carries ITS element type
//OUT: 7
//OUT: -1
//OUT: 100
//OUT: -1

// `find` is monomorphised at i64 (numbers) and u8 (the bytes of a string):
// the same body produces ?i64 hits/misses and ?u8 hits/misses.
fn find(comptime T: type, s: []T, want: T) ?T {
    for (s) |e| {
        if (e == want) {
            return e;
        }
    }
    return null;
}

pub fn main() void {
    var nums: [3]i64 = [3]i64{ 4, 7, 9 };
    if (find(i64, nums[0..3], 7)) |v| {
        print(v);
    } else {
        print(0 - 1);
    }
    if (find(i64, nums[0..3], 8)) |v| {
        print(v);
    } else {
        print(0 - 1);
    }

    var s: []u8 = "kardashev";
    // 'd' = 100 is present; 'z' = 122 is not.
    if (find(u8, s, 100)) |b| {
        print(@as(i64, b));
    } else {
        print(0 - 1);
    }
    if (find(u8, s, 122)) |b| {
        print(@as(i64, b));
    } else {
        print(0 - 1);
    }
}
