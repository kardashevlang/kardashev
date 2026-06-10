//SPEC: §22.1 every top-level item kind flattens — a struct and an enum defined in an imported file are usable in the root
//OUT: 9
//OUT: 1

@import("_types_def.ks");

pub fn main() void {
    var p: Pair = Pair{ .x = 4, .y = 5 };
    print(p.x + p.y);

    var g: Gear = Gear.High;
    switch (g) {
        .Low => { print(0); },
        .High => { print(1); },
    }
}
