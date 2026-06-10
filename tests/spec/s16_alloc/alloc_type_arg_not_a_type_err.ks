//SPEC: §16.1 alloc's second argument identifier must resolve to a type — a value binding is E0241
//ERR: E0241

pub fn main() void {
    var a: Allocator = c_allocator();
    var n: i64 = 3;
    var s: []i64 = alloc(a, n, 3); // `n` names a variable, not a type
    print(s.len);
}
