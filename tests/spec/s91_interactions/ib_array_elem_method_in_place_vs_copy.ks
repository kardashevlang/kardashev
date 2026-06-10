//SPEC: §30.2×§14 `arr[i].bump()` auto-refs the INDEX place and mutates the array element in place; copying the element first mutates only the copy
//OUT: 16
//OUT: 15
//OUT: 5

const Counter = struct {
    n: i64,
    fn bump(self: *Counter) void {
        self.n += 10;
    }
};

pub fn main() void {
    var arr: [2]Counter = [2]Counter{
        Counter{ .n = 5 },
        Counter{ .n = 6 },
    };

    arr[1].bump();        // &arr[1] — the element itself
    print(arr[1].n);      // 16

    var c: Counter = arr[0];   // array indexing copies the struct out
    c.bump();                  // &c — the copy
    print(c.n);                // 15
    print(arr[0].n);           // 5 — the element is untouched
}
