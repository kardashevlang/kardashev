//SPEC: §35.1 a wrong `@panic` argument count is E0320 (the @-builtin arity code)
//ERR: E0320

pub fn main() void {
    @panic();
}
