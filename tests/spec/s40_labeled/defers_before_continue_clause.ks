//SPEC: §40.2 `continue :L` flushes defers out to loop L FIRST, then runs L's continue-clause
//OUT: 60
//OUT: 50
//OUT: 77
//OUT: 60
//OUT: 50
//OUT: 77
//OUT: 99

pub fn main() void {
    var i: i64 = 0;
    // The continue-clause itself prints, making the ordering directly
    // observable: inner defer (60), outer body defer (50), THEN the clause (77).
    outer: while (i < 2) : (print(77)) {
        i = i + 1;
        defer print(50);
        var j: i64 = 0;
        while (j < 5) : (j = j + 1) {
            defer print(60);
            if (j == 0) {
                continue :outer;
            }
        }
    }
    print(99);
}
