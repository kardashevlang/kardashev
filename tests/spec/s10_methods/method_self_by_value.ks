//SPEC: §10 a struct function whose first parameter is `self` is a method, called `receiver.method(args)`
//OUT: 4
//OUT: 18
const Counter = struct {
    n: i32,

    fn get(self: Counter) i32 {
        return self.n;
    }

    fn plus(self: Counter, k: i32) i32 {
        return self.n + k;
    }
};

pub fn main() void {
    var c: Counter = Counter{ .n = 4 };
    print(c.get());                    // 4
    var total: i32 = 0;
    var i: i32 = 1;
    while (i <= 3) : (i += 1) {
        total = total + c.plus(i);     // (4+1) + (4+2) + (4+3)
    }
    print(total);                      // 18
}
