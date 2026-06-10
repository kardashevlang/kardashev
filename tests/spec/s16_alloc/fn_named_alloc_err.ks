//SPEC: §16.1 a user fn may not be named after the allocator builtins — `fn alloc` is E0101
//ERR: E0101

fn alloc(x: i64) i64 {
    return x;
}

pub fn main() void {
    print(alloc(1));
}
