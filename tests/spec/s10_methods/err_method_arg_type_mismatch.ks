//SPEC: §10 method argument types are checked like ordinary calls — a mismatch is E0110
//ERR: E0110
const C = struct {
    n: i32,

    fn add(self: C, k: i32) i32 {
        return self.n + k;
    }
};

pub fn main() void {
    var c: C = C{ .n = 1 };
    print(c.add(true));   // bool where i32 is expected
}
