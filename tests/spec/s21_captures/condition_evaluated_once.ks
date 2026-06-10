//SPEC: §21.1 a side-effecting `if`-capture condition runs exactly once
//OUT: 777
//OUT: 41
//OUT: 888
//OUT: -1

fn loud_some() ?i64 {
    print(777);                 // the witness: must appear exactly once
    return 40 + 1;
}

fn loud_none() ?i64 {
    print(888);
    return null;
}

pub fn main() void {
    // Were the condition re-evaluated for the unwrap, 777 would print twice.
    if (loud_some()) |n| {
        print(n);               // 41
    } else {
        print(0);
    }

    // Same on the null path: one 888, then the else arm.
    if (loud_none()) |n| {
        print(n);
    } else {
        print(0 - 1);
    }
}
