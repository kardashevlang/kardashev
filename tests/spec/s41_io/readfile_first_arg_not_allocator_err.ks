//SPEC: §41.1 `@readFile`'s first argument must be an `Allocator`
//ERR: E0321

pub fn main() void {
    var s: []u8 = @readFile(5, "x.txt");
    print(s.len);
}
