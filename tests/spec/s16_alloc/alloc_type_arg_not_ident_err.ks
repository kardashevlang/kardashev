//SPEC: §16.1 alloc's second argument must be an identifier naming a type — a literal is E0241
//ERR: E0241

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: []i64 = alloc(a, 5, 3); // 5 is not a type identifier
    print(s.len);
}
