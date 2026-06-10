//SPEC: §38 `f64` is a named type, so `?f64` works: null/value returns, if-capture, orelse
//OUT: 4.5
//OUT: 7.25

fn half(x: f64) ?f64 {
    if (x < 0.0) {
        return null;
    }
    return x / 2.0;
}

pub fn main() void {
    var p: ?f64 = half(9.0);
    if (p) |v| {
        print(v);                          // 4.5
    }
    var q: f64 = half(0.0 - 1.0) orelse 7.25;
    print(q);                              // null path -> the orelse fallback
}
