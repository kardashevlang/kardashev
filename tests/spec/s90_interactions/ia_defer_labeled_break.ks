//SPEC: §40.2 x §21.2 — `break :outer`/`continue :outer` flush defers of EVERY scope out to the target loop
//OUT: 20
//OUT: 20
//OUT: 20
//OUT: 10
//OUT: 20
//OUT: 20
//OUT: 20
//OUT: 10
//OUT: 2

// Each inner iteration registers `defer print(20)`; each outer iteration
// registers `defer print(10)`. A labeled jump from the inner body must flush
// the inner-iteration defer AND the outer-iteration defer (multi-scope),
// while plain iteration end flushes only the inner one.
pub fn main() void {
    var hits: i64 = 0;
    outer: while (true) {
        defer print(10);
        var j: i64 = 0;
        while (j < 5) : (j += 1) {
            defer print(20);
            if (j == 2) {
                hits += 1;
                if (hits == 2) {
                    break :outer;       // flushes 20, then 10, then leaves both
                }
                continue :outer;        // flushes 20, then 10, next outer iter
            }
        }
        print(99);                      // never reached: j==2 always jumps out
    }
    print(hits);
}
