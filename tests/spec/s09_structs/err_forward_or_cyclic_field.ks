//SPEC: §9.4 a struct field type may only name a *previously declared* struct — forward and cyclic references are E0160
//ERR: E0160
// `A.b` names `B` before `B` is declared (forward), and `S.next` names `S`
// itself (cyclic). Both manifestations are the same diagnostic.
const A = struct {
    b: B,
};
const B = struct {
    y: i32,
};
const S = struct {
    next: S,
};

pub fn main() void {}
