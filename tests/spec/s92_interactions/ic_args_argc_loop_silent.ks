//SPEC: §44 under a bare invocation `@argc()` is 1 (argv[0] only) and a 1..argc argument loop never runs its body
//OUT: 1

pub fn main() void {
    print(@argc());
    var a: Allocator = c_allocator();
    var i: i64 = 1;
    while (i < @argc()) : (i += 1) {
        var s: []u8 = @arg(a, i);
        print(s);
        free(a, s);
    }
}
