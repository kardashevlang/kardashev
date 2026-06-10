//SPEC: §1 identifiers are [A-Za-z_][A-Za-z0-9_]* — underscores may lead, digits and underscores may follow
//OUT: 4862
//OUT: 4863
fn _f2(n: i64) i64 {
    return n + 1;
}

pub fn main() void {
    var _x: i64 = 11;
    var x9: i64 = 13;
    var __y2: i64 = 17;
    var snake_case_2: i64 = 2;
    var product: i64 = _x * x9 * __y2 * snake_case_2;
    print(product);
    print(_f2(product));
}
