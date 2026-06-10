//SPEC: §3 an inner scope may shadow an outer binding; the outer binding is untouched after the scope ends
//OUT: 40
//OUT: 2
pub fn main() void {
    var x: i64 = 1;
    if (true) {
        var x: i64 = 10; // shadows the outer x inside this body only
        x = x * 4;
        print(x); // 40 — the inner x
    }
    x = x + 1; // the outer x was untouched by the inner scope: 1 + 1
    print(x); // 2
}
