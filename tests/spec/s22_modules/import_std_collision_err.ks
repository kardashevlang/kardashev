//SPEC: §22.1 the embedded std's items join the same global namespace — redefining `imax` while importing std is E0293
//ERR: E0293

@import("std");

pub fn imax(a: i32, b: i32) i32 {
    return a;
}

pub fn main() void {
    print(imax(1, 2));
}
