//SPEC: §2 primary := ... | "(" expr ")" — parentheses group and override precedence
//OUT: 14
//OUT: 20
//OUT: 21
pub fn main() void {
    var two: i64 = 2;
    var three: i64 = 3;
    var four: i64 = 4;
    print(two + three * four);
    print((two + three) * four);
    print(((1 + two) * (three + four)));
}
