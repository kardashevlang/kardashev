//SPEC: §10 calling a method the struct does not declare is E0170
//ERR: E0170
const P = struct {
    x: i32,

    fn get(self: P) i32 {
        return self.x;
    }
};

pub fn main() void {
    var p: P = P{ .x = 1 };
    print(p.missing());   // P declares `get`, not `missing`
}
