//SPEC: §10 a method call must bind exactly the declared non-self parameters — a count mismatch is E0171
//ERR: E0171
const C = struct {
    n: i32,

    fn add(self: C, k: i32) i32 {
        return self.n + k;
    }
};

pub fn main() void {
    var c: C = C{ .n = 1 };
    print(c.add(1, 2));   // one too many
    print(c.add());       // one too few
}
