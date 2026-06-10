//SPEC: §22.1 a LEX error inside an imported file (E0001 there) also wraps into E0294
//ERR: E0294

@import("_broken_lex.ks");

pub fn main() void {
    print(1);
}
