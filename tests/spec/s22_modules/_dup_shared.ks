// Import fixture (§22.1 uniqueness): defines `shared_name`, which the
// importing TEST file also defines — a cross-file E0293 collision.
fn shared_name() i64 {
    return 1;
}
