//SPEC: §22.1 the flattener recurses — items from a TRANSITIVELY imported file are visible to the root
//OUT: 42

// The root imports only _t_mid.ks; _t_mid.ks imports _t_leaf.ks. The root
// calls the leaf's fn directly by bare name: 6 * 7.
@import("_t_mid.ks");

pub fn main() void {
    print(mid_six() * leaf_seven());
}
