// comptime_builtins.ks — compile-time reflection builtins (v0.136).
//
//   @sizeOf(T)    -> usize   the size of a type in bytes
//   @typeName(T)  -> []u8    the source name of a type
//   @This()       a type     the enclosing struct (a portable spelling of `Self`)

const Vec3 = struct {
    x: i32,
    y: i32,
    z: i32,
    // `@This()` names this struct, so a pointer receiver mutates in place.
    fn scale(self: *@This(), k: i32) void {
        self.x *= k;
        self.y *= k;
        self.z *= k;
    }
    fn dot(self: @This(), o: Vec3) i32 {
        return self.x * o.x + self.y * o.y + self.z * o.z;
    }
};

// `@sizeOf` is substitution-aware, so it works on a generic type parameter.
fn byte_width(comptime T: type) usize {
    return @sizeOf(T);
}

pub fn main() i32 {
    print(@sizeOf(i32));        // 4
    print(@sizeOf(i64));        // 8
    print(@sizeOf(Vec3));       // 12 (three i32)
    print(byte_width(i64));     // 8 (generic)

    var v: Vec3 = Vec3{ .x = 1, .y = 2, .z = 3 };
    v.scale(10);                // -> (10, 20, 30)
    print(v.x);                 // 10
    print(v.dot(Vec3{ .x = 1, .y = 1, .z = 1 }));   // 60

    print(@typeName(i32));      // i32
    print(@typeName(Vec3));     // Vec3
    return 0;
}

test "comptime builtins" {
    expect(@sizeOf(i32) == 4);
    expect(@sizeOf(i64) == 8);
    expect(byte_width(i32) == 4);
    var v: Vec3 = Vec3{ .x = 2, .y = 0, .z = 0 };
    v.scale(3);
    expect(v.x == 6);
    expect(v.dot(Vec3{ .x = 5, .y = 9, .z = 9 }) == 30);
}
