//SPEC: §34.2 a non-member `error.X` at a `var x: Set!T = …` init site is E0330 (the init-site twin of the return-site check)
//ERR: E0330

const SmallErr = error{ A, B };

pub fn main() void {
    var x: SmallErr!i64 = error.Nope;   // Nope is not a member of SmallErr
    print(x catch 0);
}
