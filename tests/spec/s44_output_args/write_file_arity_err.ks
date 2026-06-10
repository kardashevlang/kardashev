//SPEC: §44.1 a wrong `@writeFile` argument count is the builtin-arity error
//ERR: E0320

pub fn main() void {
    var ok: bool = @writeFile("p.txt");   // takes exactly 2 arguments
    if (ok) { print(1); }
}
