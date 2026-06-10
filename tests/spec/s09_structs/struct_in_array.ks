//SPEC: §9+§14 arrays hold struct elements by value — element reads, whole-element writes, `.len`
//OUT: 44
//OUT: 6
//OUT: 30
//OUT: 4
//OUT: 3
const P = struct {
    x: i32,
    y: i32,
};

pub fn main() void {
    var arr: [3]P = [3]P{
        P{ .x = 1, .y = 2 },
        P{ .x = 3, .y = 4 },
        P{ .x = 5, .y = 6 },
    };
    var sum: i32 = 0;
    for (arr) |p| {
        sum += p.x * p.y;          // 2 + 12 + 30
    }
    print(sum);                    // 44
    print(arr[2].y);               // 6 — field read through an indexed element
    var t: P = arr[1];             // copy the element out ...
    t.x = 30;                      // ... mutate the copy ...
    arr[1] = t;                    // ... and store the whole element back
    print(arr[1].x);               // 30
    print(arr[1].y);               // 4 — the untouched field survived the round-trip
    print(arr.len);                // 3
}
