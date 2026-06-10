//SPEC: §40 `continue :outer` starts the OUTER loop's next iteration — running its continue-clause, skipping the rest of its body
//OUT: 8
//OUT: 4

pub fn main() void {
    var i: i64 = 0;
    var hits: i64 = 0;
    outer: while (i < 4) : (i = i + 1) {
        var j: i64 = 0;
        while (j < 10) : (j = j + 1) {
            if (j == 2) {
                continue :outer;     // not the inner loop's continue
            }
            hits = hits + 1;
        }
        hits = hits + 100;           // never reached: every iteration jumps out
    }
    print(hits);  // 4 outer iterations x 2 inner hits = 8 (a plain `continue`
                  // would give 4 x 10 = 40 and reach the +100 line)
    print(i);     // 4 — the outer clause ran on each `continue :outer`,
                  // otherwise the loop never terminates
}
