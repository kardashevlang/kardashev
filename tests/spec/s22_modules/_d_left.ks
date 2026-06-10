// Import fixture (§22.1 diamond): left arm over the shared base.
@import("_d_base.ks");

fn d_left() i64 {
    return d_base() + 1;
}
