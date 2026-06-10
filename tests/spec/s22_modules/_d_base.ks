// Import fixture (§22.1 diamond): the shared base, reached via BOTH _d_left.ks
// and _d_right.ks. Were it included twice, every item here would be an E0293
// duplicate — so the diamond test compiling at all pins single inclusion.
const D_TEN = 10;

fn d_base() i64 {
    return D_TEN;
}
