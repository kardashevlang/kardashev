//SPEC: §40 `break :outer` from an inner loop leaves BOTH loops at once, skipping the outer continue-clause
//OUT: 2
//OUT: 2

pub fn main() void {
    var i: i64 = 0;
    var tails: i64 = 0;
    outer: while (i < 5) : (i = i + 1) {
        var j: i64 = 0;
        while (j < 5) : (j = j + 1) {
            if (i == 2 and j == 1) {
                break :outer;
            }
        }
        tails = tails + 1;       // the code AFTER the inner loop
    }
    print(i);      // 2 — the break did NOT run the outer continue-clause
    print(tails);  // 2 — only i=0 and i=1 reached the tail; i=2 jumped past it
}
