//SPEC: §21.1 the capture binds inside `then` only — the else arm never sees it (E0100)
//ERR: E0100

pub fn main() void {
    var o: ?i64 = 5;
    if (o) |v| {
        print(v);
    } else {
        // `v` exists only in the then-block; here it is an unknown name.
        print(v);
    }
}
