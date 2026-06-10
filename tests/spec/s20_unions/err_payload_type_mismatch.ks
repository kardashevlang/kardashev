//SPEC: §20.2 the construction expression must coerce to the variant's payload type — mismatch is E0110
//ERR: E0110

const Reading = union(enum) {
    celsius: i64,
    valid: bool,
};

pub fn main() void {
    // `true` is a bool; the `celsius` payload is i64 — no coercion exists.
    var r: Reading = Reading{ .celsius = true };
    switch (r) {
        .celsius => |c| {
            print(c);
        },
        .valid => {
            print(1);
        },
    }
}
