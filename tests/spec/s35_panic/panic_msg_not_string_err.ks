//SPEC: §35.1 the `@panic` argument must be a `[]u8` — a non-string message is a type error (E0110)
//ERR: E0110

pub fn main() void {
    @panic(42);
}
