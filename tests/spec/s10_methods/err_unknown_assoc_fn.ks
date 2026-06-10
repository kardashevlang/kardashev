//SPEC: §10 calling an associated function the struct does not declare is E0170
//ERR: E0170
const P = struct {
    x: i32,

    fn zero() P {
        return P{ .x = 0 };
    }
};

pub fn main() void {
    print(P.nothing());   // P declares `zero`, not `nothing`
}
