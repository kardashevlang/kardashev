//SPEC: §2 func := "pub"? "fn" IDENT "(" params? ")" type block — non-pub functions, multi-params with a trailing comma, and void functions
//OUT: 7
//OUT: 6
fn add3(
    a: i64,
    b: i64,
    c: i64,
) i64 {
    return a + b + c;
}

fn shout() void {
    print(7);
    return;
}

pub fn main() void {
    shout();
    print(add3(1, 2, 3));
}
