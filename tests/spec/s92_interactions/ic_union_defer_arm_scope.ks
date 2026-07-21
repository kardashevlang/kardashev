//SPEC: §4.4 x §20 a `defer` inside a switch arm flushes at that arm's end, before statements after the switch
//OUT: 1
//OUT: 2
//OUT: 3

const U = union(enum) { a: i64 };

pub fn main() void {
    var u: U = U{ .a = 1 };
    switch (u) {
        .a => |v| {
            defer print(2);
            print(v);
        },
    }
    print(3);
}
