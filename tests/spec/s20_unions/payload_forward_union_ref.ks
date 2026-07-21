//SPEC: §20.2 union payload types resolve after ALL union names intern: a variant may reference a union declared later
//OUT: 11
//OUT: 4

const Outer = union(enum) { wrap: Inner, flat: i64 };
const Inner = union(enum) { n: i64 };

pub fn main() void {
    var o: Outer = Outer{ .wrap = Inner{ .n = 11 } };
    switch (o) {
        .wrap => |i| {
            switch (i) {
                .n => |v| { print(v); },
            }
        },
        .flat => |v| { print(v); },
    }
    var f: Outer = Outer{ .flat = 4 };
    switch (f) {
        .wrap => { print(0); },
        .flat => |v| { print(v); },
    }
}
