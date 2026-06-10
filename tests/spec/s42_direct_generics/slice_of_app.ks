//SPEC: §42.1 `[]Name(A)` — a slice of an application; element reads and writes alias the backing array
//OUT: 14
//OUT: 100

// `[]Box(i32)` as a parameter type, produced by slicing a `[2]Box(i32)`.
// (Element METHOD receivers via `s[i].m()` are quarantined — a pre-§42 emit
// bug; fields and local copies are the supported element access here.)
fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
        fn get(self: Self) T {
            return self.v;
        }
    };
}

fn sum(s: []Box(i32)) i32 {
    var t: i32 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        var e: Box(i32) = s[i];        // copy out, then call the method
        t = t + e.get() + s[i].v;      // field access on the element works
    }
    return t;
}

pub fn main() void {
    var arr: [2]Box(i32) = [2]Box(i32){ Box(i32).init(3), Box(i32).init(4) };
    var s: []Box(i32) = arr[0..2];
    print(sum(s));                     // (3+3) + (4+4) = 14
    s[1] = Box(i32).init(100);         // write through the slice...
    print(arr[1].v);                   // ...is visible in the array
}
