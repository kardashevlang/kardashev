// enum_values.ks — enum explicit values + int conversions (v0.143).
//
//   const E = enum { A = 1, B, C = 10 };   // B auto-increments to 2
//   @intFromEnum(e)   -> i64   the variant's integer value
//   @enumFromInt(E, n) -> E    the variant with value n
//
// Explicit values give enums a stable wire/ABI representation (HTTP status
// codes, syscall numbers, protocol tags) and round-trip through integers.

const Status = enum {
    Ok = 200,
    NotFound = 404,
    Teapot = 418,
    ServerError = 500,
};

// Map a status to a coarse category (1xx..5xx) using its integer value.
fn category(s: Status) i32 {
    return @as(i32, @intFromEnum(s)) / 100;
}

const Dir = enum { North, East, South, West };   // auto 0,1,2,3

// Turn right: (d + 1) mod 4, via the integer round-trip.
fn turn_right(d: Dir) Dir {
    var n: i32 = @as(i32, @intFromEnum(d));
    return @enumFromInt(Dir, @as(i64, (n + 1) % 4));
}

pub fn main() i32 {
    print(@intFromEnum(Status.NotFound));   // 404
    print(@intFromEnum(Status.Teapot));     // 418
    print(category(Status.Ok));             // 2
    print(category(Status.ServerError));    // 5

    var d: Dir = Dir.North;
    print(@intFromEnum(d));                  // 0
    d = turn_right(d);                       // East
    print(@intFromEnum(d));                  // 1
    d = turn_right(turn_right(d));           // West
    print(@intFromEnum(d));                  // 3
    return 0;
}

test "enum values" {
    expect(@intFromEnum(Status.Ok) == 200);
    expect(@intFromEnum(Status.ServerError) == 500);
    expect(category(Status.Teapot) == 4);
    expect(@intFromEnum(turn_right(Dir.West)) == 0);   // West -> North wraps
    var s: Status = @enumFromInt(Status, 404);
    expect(@intFromEnum(s) == 404);
}
