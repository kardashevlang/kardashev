//SPEC: §30.2 auto-ref of a pointer-receiver call needs an addressable lvalue — a temporary (a call result) as receiver is rejected, as for `&`
//ERR: E0231
const P = struct {
    x: i64,

    fn inc(self: *P) void {
        self.x += 1;
    }
};

fn make() P {
    return P{ .x = 5 };
}

pub fn main() void {
    make().inc();      // the receiver is an rvalue temporary: no address to take
}
