//SPEC: §2 if_stmt := "if" "(" expr ")" block ("else" (if_stmt | block))? — else-if chains select exactly one branch
//OUT: -1
//OUT: 0
//OUT: 1
//OUT: 2
fn classify(n: i64) i64 {
    if (n < 0) {
        return -1;
    } else if (n == 0) {
        return 0;
    } else if (n < 100) {
        return 1;
    } else {
        return 2;
    }
}

pub fn main() void {
    print(classify(0 - 5));
    print(classify(0));
    print(classify(42));
    print(classify(1000));
}
