//SPEC: §12 x §38 `!f64`: try propagates the double payload, catch recovers with a double default
//OUT: 3.5
//OUT: 0.25

fn half(x: f64, ok: bool) !f64 {
    if (ok) { return x / 2.0; }
    return error.Bad;
}

fn run(ok: bool) !f64 {
    var v: f64 = try half(7.0, ok);
    return v;
}

pub fn main() void {
    print(run(true) catch 0.25);    // 3.5
    print(run(false) catch 0.25);   // the fallback
}
