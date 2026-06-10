//SPEC: §42.2 a slice may be typed over an alias OR a direct application element (`[]IB` / `[]Box(i64)`) — and `s[i].m()` resolves the instance method either way
//OUT: 7
//OUT: 7

fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn get(self: Self) T {
            return self.v;
        }
    };
}

const IB = Box(i64);

fn sum_alias(s: []IB) i64 {
    var t: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        t = t + s[i].get();
    }
    return t;
}

fn sum_app(s: []Box(i64)) i64 {
    var t: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        t = t + s[i].get();
    }
    return t;
}

pub fn main() void {
    var arr: [2]IB = [2]IB{ IB{ .v = 3 }, IB{ .v = 4 } };
    print(sum_alias(arr[0..2]));
    print(sum_app(arr[0..2]));
}
