//SPEC: §16.1 `alloc`'s literal type argument does not accept a union name (a subst-bound type parameter does)
//ERR: E0241

const U = union(enum) { a: i64 };

pub fn main() void {
    var al: Allocator = c_allocator();
    var s: []U = alloc(al, U, 3);
    free(al, s);
}
