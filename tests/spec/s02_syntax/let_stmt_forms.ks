//SPEC: §2/§18 let_stmt: `var`/`const` locals with a type annotation, or without one (type inferred from the initializer, integers default i64)
//OUT: 42
//OUT: 50
//OUT: 7
pub fn main() void {
    var a: i64 = 6;
    const b: i64 = 7;
    var c = a * b;     // inferred i64 = 42
    const d = c + 8;   // inferred i64 = 50
    a = a + 1;         // assign_stmt: only a `var` target
    print(c);
    print(d);
    print(a);
}
