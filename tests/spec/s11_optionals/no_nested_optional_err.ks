//SPEC: §11 no nesting: `??T` is rejected at parse ("expected identifier" after `?`, E0200)
//ERR: E0200

pub fn main() void {
    var x: ??i64 = null;
}
