//SPEC: §41.1 a wrong `@readLine` argument count is the builtin-arity error
//ERR: E0320

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: []u8 = @readLine(a, a);   // takes exactly 1 argument
    print(s.len);
}
