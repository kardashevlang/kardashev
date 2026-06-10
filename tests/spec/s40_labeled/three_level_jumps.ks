//SPEC: §40 labeled jumps from a THIRD nesting level exit exactly to their target loop
//OUT: 113111
//OUT: 2

pub fn main() void {
    // log appends a digit per checkpoint: 1 = innermost body, 2 = after the
    // innermost loop (inside b), 3 = after loop b (inside a).
    var log: i64 = 0;
    var n: i64 = 0;
    a: while (n < 9) : (n = n + 1) {
        var m: i64 = 0;
        b: while (m < 3) : (m = m + 1) {
            var p: i64 = 0;
            while (p < 3) : (p = p + 1) {
                log = log * 10 + 1;
                if (n == 0 and p == 1) {
                    break :b;        // exits 2 loops -> "3" runs, "2" must NOT
                }
                if (n == 1 and p == 0) {
                    continue :a;     // skips BOTH tails, runs a's clause
                }
                if (n == 2 and p == 1) {
                    break :a;        // exits all 3 loops, no tails, no clause
                }
            }
            log = log * 10 + 2;
        }
        log = log * 10 + 3;
    }
    // n=0: 1,1 then break :b -> 3 | n=1: 1 then continue :a | n=2: 1,1 break :a
    print(log);   // 11 3 1 11 -> 113111
    print(n);     // 0->1 (body end), 1->2 (continue :a); break :a kept n=2
}
