// Import fixture (§22.1): imports the embedded std itself — the root ALSO
// imports std, pinning that `<std>` dedups like any other module (no E0293).
@import("std");

fn helper_min() i32 {
    return imin(4, 2);
}
