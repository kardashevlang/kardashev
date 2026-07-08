//SPEC: §24.3 a NEGATIVE comptime value argument instantiates and mangles as `m<digits>` — `-` is not a C identifier character (v0.178)
//OUT: 7
//OUT: 13
//OUT: -17

// Each distinct value — negative included — is its own monomorphised
// instance; `addk(-3, …)` and `addk(3, …)` must not collide, and the
// negative instance's C name (`kd_addk__m3`) must compile.
fn addk(comptime k: i64, x: i64) i64 {
    return x + k;
}

pub fn main() void {
    print(addk(-3, 10));
    print(addk(3, 10));
    print(addk(-20, 3));
}
