//SPEC: §40.2 `break :outer` from an inner loop leaves BOTH loops without running the outer continue-clause or the outer body tail
//OUT: 0
//OUT: 1

pub fn main() void {
    var i: i64 = 0;
    var hits: i64 = 0;
    outer: while (i < 5) : (i += 10) {
        hits += 1;
        var j: i64 = 0;
        while (j < 5) : (j += 1) {
            if (j == 2) {
                break :outer;
            }
        }
        hits += 100;     // never reached: break leaves the outer body too
    }
    print(i);            // 0 — the `i += 10` clause never ran
    print(hits);         // 1 — exactly one (partial) outer pass
}
