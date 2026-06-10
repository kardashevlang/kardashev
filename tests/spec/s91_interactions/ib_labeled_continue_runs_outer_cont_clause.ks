//SPEC: §40.2 `continue :outer` from an inner loop skips the rest of the OUTER body and RUNS the outer while's continue-clause before re-testing
//OUT: 1000
//OUT: 0
//OUT: 1001
//OUT: 10
//OUT: 1002
//OUT: 20
//OUT: 3

pub fn main() void {
    var i: i64 = 0;
    outer: while (i < 3) : (i += 1) {
        print(1000 + i);
        var j: i64 = 0;
        while (j < 10) : (j += 1) {
            if (j == 1) {
                continue :outer;     // must run `i += 1` (else this never ends)
            }
            print(i * 10 + j);
        }
        print(0 - 999);              // the outer body tail is skipped
    }
    print(i);                        // 3: the clause ran every pass
}
