//SPEC: §11.2 a `T` value widens to `?T` on assignment to a `?T` place
//OUT: -7
//OUT: 1
//OUT: 4
//OUT: 9
//OUT: 16
//OUT: -7

// A `?i64` variable starts null, is repeatedly assigned plain i64 values
// computed in a loop (each assignment is a T -> ?T widening), and is finally
// reset to null. `orelse` witnesses the stored payload after every step.
pub fn main() void {
    var slot: ?i64 = null;
    print(slot orelse 0 - 7);
    var i: i64 = 1;
    while (i <= 4) : (i = i + 1) {
        slot = i * i;               // plain i64 assigned to a ?i64 place
        print(slot orelse 0 - 7);
    }
    slot = null;                    // and back to empty
    print(slot orelse 0 - 7);
}
