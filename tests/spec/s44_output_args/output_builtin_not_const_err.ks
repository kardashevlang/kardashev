//SPEC: §44.1 the output/argv builtins are runtime-only — `const_eval` rejects them in a `const` initializer
//ERR: E0130

const N: i64 = @argc();

pub fn main() void {
    print(N);
}
