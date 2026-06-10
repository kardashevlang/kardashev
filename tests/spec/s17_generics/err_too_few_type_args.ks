//SPEC: §17.2 too few type arguments for a generic-function call is E0252
//ERR: E0252

fn first(comptime T: type, a: T, b: T) T {
    if (a < b) {
        return a;
    }
    return b;
}

pub fn main() void {
    // One comptime parameter to bind, zero arguments supplied.
    print(first());
}
