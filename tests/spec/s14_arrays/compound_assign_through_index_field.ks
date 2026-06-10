//SPEC: §27.3+§14.1 a compound assignment whose place passes through an index (`arr[i].f += e`) evaluates the place ONCE and writes the element's field
//OUT: 7
//OUT: 15
//OUT: 60
//OUT: 20
const P = struct {
    x: i64,
};

fn pick() i64 {
    print(7);          // must appear exactly once: the place is evaluated once
    return 1;
}

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 10 }, P{ .x = 20 } };
    arr[pick()].x += 5;        // prints 7 once; arr[1].x = 25
    arr[0].x += 5;             // 15
    print(arr[0].x);           // 15
    arr[0].x *= 4;             // 60
    print(arr[0].x);           // 60
    var s: []P = arr[0..2];
    s[1].x -= 5;               // 25 - 5 = 20, through the slice view
    print(arr[1].x);           // 20
}
