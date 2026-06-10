//SPEC: §11.1 `?` wraps a named type only: a composite inner like `?[]u8` is rejected at parse ("expected identifier", E0200)
//ERR: E0200

pub fn main() void {
    var s: ?[]u8 = null;
}
