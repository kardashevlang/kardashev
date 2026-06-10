//SPEC: §11.2 a struct field of type `?T` is an assignable optional place (widening write + null reset)
//OUT: 4
//OUT: 25
//OUT: -1

const Cache = struct {
    last: ?i64,
    hits: i64,
};

// A memo that compares each new square against the previous one through its
// optional field; the field place takes plain i64 writes (widening) and a
// final null reset.
pub fn main() void {
    var c: Cache = Cache{ .last = null, .hits = 0 };
    var i: i64 = 1;
    while (i <= 5) : (i = i + 1) {
        if (c.last) |prev| {
            if (prev == i * i - 2 * i + 1) {   // prev == (i-1)^2
                c.hits = c.hits + 1;
            }
        }
        c.last = i * i;     // plain i64 into the ?i64 field place
    }
    print(c.hits);                  // matched on i = 2,3,4,5 -> 4
    print(c.last orelse 0 - 1);     // 25
    c.last = null;                  // reset the field place to empty
    print(c.last orelse 0 - 1);     // -1
}
