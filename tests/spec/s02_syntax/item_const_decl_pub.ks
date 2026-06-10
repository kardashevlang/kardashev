//SPEC: §2 const_decl := "pub"? "const" IDENT ":" type "=" expr ";" — top-level constants, usable in later initializers
//OUT: 35
//OUT: 49
pub const BASE: i64 = 7;
const SCALE: i64 = BASE * 6;

pub fn main() void {
    print(SCALE - BASE);
    print(BASE * BASE);
}
