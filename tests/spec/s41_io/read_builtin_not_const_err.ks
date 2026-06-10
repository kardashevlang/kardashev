//SPEC: §41.1 the read builtins are not constant — `const_eval` rejects them in a `const` initializer
//ERR: E0130

const L: []u8 = @readLine(c_allocator());

pub fn main() void {
    print(L.len);
}
