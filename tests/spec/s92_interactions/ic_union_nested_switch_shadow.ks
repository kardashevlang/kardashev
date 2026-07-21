//SPEC: §20.1 nested union switches: an inner capture shadows an outer capture of the same name inside the inner arm only
//OUT: 3
//OUT: 5
//OUT: 3

const W = union(enum) { n: i64 };

pub fn main() void {
    var outer: W = W{ .n = 3 };
    var inner: W = W{ .n = 5 };
    switch (outer) {
        .n => |v| {
            print(v);   // 3
            switch (inner) {
                .n => |v| { print(v); },   // the inner capture shadows: 5
            }
            print(v);   // the outer capture is visible again: 3
        },
    }
}
