//SPEC: §44 `@argc()` counts argv[0] (the executable name) — it is always ≥ 1
//OUT: 1

pub fn main() void {
    if (@argc() >= 1) { print(1); } else { print(0); }
}
