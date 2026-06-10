//SPEC: §2 call := IDENT "(" args? ")", args := expr ("," expr)* ","? — zero-arg calls, nested calls, trailing comma, and a call as an expression statement
//OUT: 1
//OUT: 15
//OUT: 5
fn five() i64 {
    return 5;
}

fn twice(n: i64) i64 {
    return n * 2;
}

fn plus(a: i64, b: i64) i64 {
    return a + b;
}

fn ping() void {
    print(1);
}

pub fn main() void {
    ping();
    print(plus(twice(3), plus(4, 5,),));
    print(five());
}
