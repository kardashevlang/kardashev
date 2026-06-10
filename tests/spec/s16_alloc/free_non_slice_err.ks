//SPEC: §16.1 free's second argument must be a slice `[]T` — anything else is E0242
//ERR: E0242

pub fn main() void {
    var a: Allocator = c_allocator();
    var x: i64 = 42;
    free(a, x); // an i64 is not a slice
}
