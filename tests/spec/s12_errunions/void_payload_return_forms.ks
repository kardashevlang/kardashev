//SPEC: §12.1 a `!void` function returns success via `return;`, by falling off its end, or by `return <call returning !void>;`
//OUT: 1
//OUT: 2
//OUT: 5
//OUT: 101

fn step(n: i64) !void {
    if (n > 5) {
        return error.TooBig;
    }
    if (n < 0) {
        return; // explicit success return — no payload value to supply
    }
    print(n);
    // implicit success: falls off the end
}

fn run(n: i64) !void {
    try step(n);
    return step(n + 1); // `return <!void call>;` passes the union through
}

pub fn main() void {
    run(1) catch print(0 - 1);     // 1, 2 — success, handler not run
    run(0 - 3) catch print(0 - 2); // both steps `return;` — nothing printed
    run(5) catch |e| print(100 + e); // 5, then step(6) fails -> 100+1
}
