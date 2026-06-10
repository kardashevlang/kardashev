//SPEC: §21.1 the condition is evaluated once into a temp — the capture is a value copy of it
//OUT: 5
//OUT: -1

pub fn main() void {
    var o: ?i64 = 2 + 3;

    if (o) |v| {
        // Nulling the SOURCE inside then must not disturb the capture: `v`
        // was copied out of the already-evaluated temp.
        o = null;
        print(v);               // still 5
    } else {
        print(0);
    }

    // ...but the write to `o` itself did happen.
    if (o) |v| {
        print(v);
    } else {
        print(0 - 1);           // o is null now
    }
}
