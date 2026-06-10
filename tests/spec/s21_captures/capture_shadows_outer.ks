//SPEC: §21.1 the capture's binding scope wraps only `then` — it shadows an outer name there and is gone after
//OUT: 50
//OUT: 7

pub fn main() void {
    var v: i64 = 7;                 // outer `v`
    var o: ?i64 = 49 + 1;

    if (o) |v| {
        // Inside then, `v` is the capture (50), shadowing the outer 7.
        print(v);
    } else {
        print(0 - 1);
    }

    // After the if, the capture scope is gone: `v` is the outer binding again,
    // and the shadowing never wrote through to it.
    print(v);
}
