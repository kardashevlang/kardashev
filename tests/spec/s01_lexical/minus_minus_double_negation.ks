//SPEC: §1 there is no `--` token: `a--b` lexes `-` `-` and means a - (-b); `=-`/`-=` split per maximal munch
//OUT: 13
//OUT: 10
//OUT: -5
//OUT: 2
pub fn main() void {
    print(10--3);
    print(5--3--2);
    var z: i64 = 0;
    // `=-` is `=` then unary `-`: assigns negative five.
    z =- 5;
    print(z);
    // `-=` IS one token (§27); with a negated rhs: z = -5 - (-7) = 2.
    z -= -7;
    print(z);
}
