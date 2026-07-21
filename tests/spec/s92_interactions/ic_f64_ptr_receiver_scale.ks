//SPEC: §30 x §38 a pointer-receiver method mutates an f64 field in place
//OUT: 2.5
//OUT: 7.5

const Meter = struct {
    v: f64,
    fn scale(self: *Meter, by: f64) void {
        self.v = self.v * by;
    }
};

pub fn main() void {
    var m: Meter = Meter{ .v = 2.5 };
    print(m.v);
    m.scale(3.0);
    print(m.v);   // 7.5 — the mutation landed in the caller's struct
}
