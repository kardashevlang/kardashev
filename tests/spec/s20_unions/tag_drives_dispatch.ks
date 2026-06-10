//SPEC: §20.3 the tag (not the payload) drives `switch` dispatch — equal payloads, different variants
//OUT: 130
//OUT: 100

const Tx = union(enum) {
    credit: i64,
    debit: i64,
};

fn apply(balance: i64, t: Tx) i64 {
    switch (t) {
        .credit => |v| {
            return balance + v;
        },
        .debit => |v| {
            return balance - v;
        },
    }
}

pub fn main() void {
    // The SAME payload value flows into both variants; only the tag differs,
    // so a tag mix-up would make the two results coincide or swap.
    var amt: i64 = 10 * 3;
    var bal: i64 = 100;
    bal = apply(bal, Tx{ .credit = amt });      // 130
    print(bal);
    bal = apply(bal, Tx{ .debit = amt });       // 100
    print(bal);
}
