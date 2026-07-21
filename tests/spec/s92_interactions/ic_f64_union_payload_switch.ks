//SPEC: §20 x §38 an f64 union payload beside an i64 sibling: each capture prints its own scalar kind
//OUT: 6.25
//OUT: 40

const M = union(enum) { d: f64, n: i64 };

pub fn main() void {
    var xs: [2]M = [2]M{ M{ .d = 6.25 }, M{ .n = 40 } };
    for (xs) |m| {
        switch (m) {
            .d => |v| { print(v); },
            .n => |v| { print(v); },
        }
    }
}
