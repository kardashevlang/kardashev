// generics.ks — comptime generics (v0.120).
//
// A `comptime T: type` parameter makes a function generic. Each distinct type
// argument monomorphises into its own specialised function — no runtime
// dispatch, no boxing. Type arguments are passed positionally: `max(i32, a, b)`.

fn max(comptime T: type, a: T, b: T) T {
    if (a > b) {
        return a;
    }
    return b;
}

fn min(comptime T: type, a: T, b: T) T {
    if (a < b) {
        return a;
    }
    return b;
}

fn clamp(comptime T: type, x: T, lo: T, hi: T) T {
    return min(T, max(T, x, lo), hi);   // generics calling generics; T forwarded
}

pub fn main() i32 {
    print(max(i32, 3, 9));          // 9
    print(min(i32, 3, 9));          // 3
    print(clamp(i32, 15, 0, 10));   // 10
    print(clamp(i32, 0 - 4, 0, 10)); // 0
    print(clamp(i32, 7, 0, 10));    // 7
    return 0;
}

test "max/min/clamp" {
    expect(max(i32, 5, 2) == 5);
    expect(min(i64, 5, 2) == 2);        // i64 instantiation
    expect(clamp(i32, 100, 1, 9) == 9);
    expect(clamp(i32, 5, 1, 9) == 5);
}
