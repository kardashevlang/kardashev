//SPEC: §40.2 `break :L` flushes defers from the innermost scope out to AND INCLUDING loop L's body scope, LIFO
//OUT: 1
//OUT: 40
//OUT: 30
//OUT: 20
//OUT: 30
//OUT: 20
//OUT: 10
//OUT: 2

pub fn main() void {
    print(1);
    var i: i64 = 0;
    outer: while (i < 9) : (i = i + 1) {
        defer print(10);             // outer loop-body scope
        var j: i64 = 0;
        while (j < 3) : (j = j + 1) {
            defer print(20);         // inner loop-body scope
            {
                defer print(30);     // plain block scope
                if (j == 1) {
                    break :outer;    // crosses 3 scopes: 30, 20, 10 in order
                }
                print(40);
            }
        }
    }
    print(2);
    // j=0 runs normally: 40, then 30 (block exit), 20 (inner body exit).
    // j=1 hits the labeled break: 30, 20, 10 — strictly innermost-out — then
    // control lands AFTER the outer loop (no second outer iteration).
}
