//SPEC: §10 a struct function without `self` is an associated function, called `Type.f(args)`
//OUT: 34
//OUT: 0
//OUT: 14
const Vec = struct {
    x: i32,
    y: i32,

    fn make(x: i32, y: i32) Vec {
        return Vec{ .x = x, .y = y };
    }

    fn zero() Vec {
        return Vec{ .x = 0, .y = 0 };
    }
};

pub fn main() void {
    var v: Vec = Vec.make(3, 4);
    print(v.x * 10 + v.y);             // 34
    var z: Vec = Vec.zero();
    print(z.x + z.y);                  // 0
    var sum: i32 = 0;
    var i: i32 = 1;
    while (i <= 3) : (i += 1) {
        sum = sum + Vec.make(i, i * i).y;   // 1 + 4 + 9
    }
    print(sum);                        // 14
}
