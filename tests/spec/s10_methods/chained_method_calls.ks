//SPEC: §10 method calls chain left-to-right — `c.m(1).m(2)` applies each call to the previous result
//OUT: 30
//OUT: 12
//OUT: 1
const Cnt = struct {
    n: i32,

    fn bumped(self: Cnt, by: i32) Cnt {
        return Cnt{ .n = self.n + by };
    }

    fn scaled(self: Cnt, k: i32) Cnt {
        return Cnt{ .n = self.n * k };
    }

    fn get(self: Cnt) i32 {
        return self.n;
    }
};

pub fn main() void {
    var c: Cnt = Cnt{ .n = 1 };
    print(c.bumped(2).scaled(10).get());   // (1+2)*10 = 30
    print(c.scaled(10).bumped(2).get());   // 1*10+2 = 12 — order is left-to-right
    print(c.get());                        // 1 — each link returned a fresh value
}
