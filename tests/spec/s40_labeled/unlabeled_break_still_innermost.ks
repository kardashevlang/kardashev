//SPEC: §40 an UNLABELED `break` inside a labeled loop still targets only the innermost loop
//OUT: 24
//OUT: 2

pub fn main() void {
    var spins: i64 = 0;
    var i: i64 = 0;
    outer: while (i < 3) : (i = i + 1) {
        var j: i64 = 0;
        while (j < 100) : (j = j + 1) {
            if (j == 2) {
                break;              // innermost only — the outer label is inert
            }
            if (i == 2) {
                break :outer;
            }
            spins = spins + 1;
        }
        spins = spins + 10;         // reached after the PLAIN break
    }
    print(spins);  // i=0: 2+10, i=1: 2+10, i=2: labeled break before anything
    print(i);      // 2
}
