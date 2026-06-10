//SPEC: §30.2 an associated call `Type.method(args)` passes its explicit arguments unchanged — an explicit `&p` for a pointer receiver mutates, an explicit value `self` is a copy
//OUT: 10
//OUT: 10
//OUT: 7
const K = struct {
    n: i64,

    fn bump(self: *K, by: i64) void {
        self.n += by;
    }

    fn read(self: K) i64 {
        return self.n;
    }
};

pub fn main() void {
    var k: K = K{ .n = 1 };
    K.bump(&k, 9);         // static form: the caller supplies &k itself
    print(k.n);            // 10 — the mutation landed on k
    print(K.read(k));      // 10 — static form with an explicit value self
    K.bump(&k, 0 - 3);
    print(K.read(k));      // 7
}
