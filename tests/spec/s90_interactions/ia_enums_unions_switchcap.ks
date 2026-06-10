//SPEC: §37 x §20 — an explicit-valued enum as a union PAYLOAD: switch-capture hands it to @intFromEnum
//OUT: 54
//OUT: 2

// Node holds either a number or a token; the token enum carries explicit
// values (1, 5, 9). The union switch captures each payload and the enum arm
// converts through @intFromEnum — broken values would change the sum.
const Tok = enum { Word = 1, Num = 5, End = 9 };

const Node = union(enum) {
    leaf: i64,
    tok: Tok,
};

pub fn main() void {
    var ns: [3]Node = [3]Node{
        Node{ .leaf = 40 },
        Node{ .tok = Tok.Num },
        Node{ .tok = Tok.End },
    };
    var total: i64 = 0;
    var toks: i64 = 0;
    for (ns) |n| {
        switch (n) {
            .leaf => |v| { total += v; },
            .tok => |t| {
                total += @intFromEnum(t);   // 5 then 9
                toks += 1;
            },
        }
    }
    print(total);   // 40 + 5 + 9 = 54
    print(toks);    // 2
}
