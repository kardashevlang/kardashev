//SPEC: §15.1 a `*T` parameter lets a function mutate the caller's local through the pointer
//OUT: 55

// `acc` is only ever modified inside `add_into`; if pointer parameters copied
// the pointee the final value would still be 0.
fn add_into(p: *i64, v: i64) void {
    p.* = p.* + v;
}

pub fn main() void {
    var acc: i64 = 0;
    var k: i64 = 1;
    while (k <= 10) : (k += 1) {
        add_into(&acc, k);
    }
    print(acc); // 1 + 2 + ... + 10
}
