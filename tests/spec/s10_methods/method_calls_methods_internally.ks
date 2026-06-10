//SPEC: §10 method bodies call other methods via `self.m()` and associated functions via `Type.f()`
//OUT: 20
//OUT: -6
const Temp = struct {
    deg: i32,

    fn double_deg(self: Temp) i32 {
        return self.deg * 2;
    }

    fn quad(self: Temp) i32 {
        return self.double_deg() * 2;     // self.method() inside a method
    }

    fn base() Temp {
        return Temp{ .deg = 0 };
    }

    fn from(d: i32) Temp {
        var t: Temp = Temp.base();        // assoc fn calling another assoc fn
        t.deg = d;
        return t;
    }
};

pub fn main() void {
    print(Temp.from(5).quad());           // 5*2*2 = 20
    print(Temp.from(0 - 3).double_deg()); // -6
}
