//SPEC: §9.4 a field-assignment place must be rooted in an assignable `var` — a `const` or param root is E0167
//ERR: E0167
const P = struct {
    x: i32,
};

fn poke(p: P) void {
    p.x = 2;                      // param root — params are immutable
}

pub fn main() void {
    const frozen: P = P{ .x = 1 };
    frozen.x = 5;                 // const root
    poke(frozen);
}
