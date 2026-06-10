//SPEC: §30 value and pointer receivers coexist on one struct; a pointer-receiver body may call a value-receiver method on its own `*Self` self (auto-deref)
//OUT: 7
//OUT: 7
//OUT: 8
//OUT: 9
const Ctr = struct {
    n: i64,

    fn peek(self: Ctr) i64 {
        return self.n;
    }

    fn bump(self: *Ctr) i64 {
        var before: i64 = self.peek();   // value method on the pointer self: auto-deref copy
        self.n += 1;                     // true mutation after the copy was taken
        return before;
    }
};

pub fn main() void {
    var c: Ctr = Ctr{ .n = 7 };
    print(c.peek());   // 7
    print(c.bump());   // 7 — `before` predates the increment
    print(c.bump());   // 8 — the previous bump really persisted
    print(c.peek());   // 9
}
