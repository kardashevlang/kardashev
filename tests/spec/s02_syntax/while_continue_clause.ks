//SPEC: §2 while_stmt := "while" "(" expr ")" (":" "(" loop_cont ")")? block — the continue-clause runs after each iteration, including on `continue`
//OUT: 13
//OUT: 6
pub fn main() void {
    var i: i64 = 0;
    var sum: i64 = 0;
    while (i < 6) : (i = i + 1) {
        if (i == 2) {
            // If the clause did not run on `continue`, this would loop forever.
            continue;
        }
        sum = sum + i;
    }
    // 0+1+3+4+5 = 13, and the clause left i at 6.
    print(sum);
    print(i);
}
