//SPEC: §18.2 an un-annotated binding adopts its initializer's type; an integer literal defaults to `i64`
//OUT: 9223372036854775807
//OUT: 3
pub fn main() void {
    var big = 4611686018427387903; // 2^62 - 1: representable only in i64
    var twice = big + big;         // 2^63 - 2 = 9223372036854775806
    var more: i64 = twice + 1;     // mixes with an annotated i64 — same-type
                                   // arithmetic proves the inferred type is i64
    print(more);

    var q = 7 / 2; // both literals default to i64: integer division → 3
    print(q);      // (an f64 default would print 3.5)
}
