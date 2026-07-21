//SPEC: §11 x §38 `?f64`: widening at init, orelse picks the double default, the if-capture unwraps for arithmetic
//OUT: 1.25
//OUT: 0.5
//OUT: 9.75

pub fn main() void {
    var p: ?f64 = 1.25;   // widened f64 → ?f64
    var q: ?f64 = null;
    print(p orelse 0.5);   // has a value → 1.25
    print(q orelse 0.5);   // null → the default
    if (p) |v| {
        print(v + 8.5);    // 9.75
    } else {
        print(0.0);
    }
}
