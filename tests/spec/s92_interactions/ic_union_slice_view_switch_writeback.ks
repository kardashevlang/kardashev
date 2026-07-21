//SPEC: §20 x §15.2 a slice view over a `[N]Union` array: for + switch-capture reads elements, an indexed write through the view lands in the backing array
//OUT: 40
//OUT: 2
//OUT: 99

const Node = union(enum) { leaf: i64, pair: [2]i64 };

pub fn main() void {
    var xs: [3]Node = [3]Node{
        Node{ .leaf = 40 },
        Node{ .pair = [2]i64{ 1, 1 } },
        Node{ .leaf = 7 },
    };
    var s: []Node = xs[0..3];
    var total: i64 = 0;
    var pairs: i64 = 0;
    for (s) |n| {
        switch (n) {
            .leaf => |v| {
                if (v == 40) { total += v; }
            },
            .pair => |p| { pairs += p[0] + p[1]; },
        }
    }
    print(total);   // only the 40 leaf passes the filter
    print(pairs);   // 1 + 1
    s[2] = Node{ .leaf = 99 };
    switch (xs[2]) {
        .leaf => |v| { print(v); },   // 99 — the view write hit the array
        .pair => { print(0); },
    }
}
