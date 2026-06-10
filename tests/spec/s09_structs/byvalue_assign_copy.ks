//SPEC: §9 structs are by-value — `var q = p` copies, so mutating the copy leaves the original
//OUT: 10
//OUT: 40
//OUT: 20
//OUT: 99
const P = struct {
    x: i32,
    y: i32,
};

pub fn main() void {
    var p: P = P{ .x = 10, .y = 20 };
    var q: P = p;                  // a full copy, not an alias
    var i: i32 = 0;
    while (i < 3) : (i += 1) {
        q.x = q.x + p.x;           // 10 -> 20 -> 30 -> 40
    }
    print(p.x);                    // 10 — untouched by the loop on q
    print(q.x);                    // 40
    p.y = 99;                      // independence holds in both directions
    print(q.y);                    // 20
    print(p.y);                    // 99
}
