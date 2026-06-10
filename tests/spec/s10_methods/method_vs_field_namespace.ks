//SPEC: §10 postfix `.name(` parses as a method call while `.name` is field access — the trailing `(` disambiguates
//OUT: 4
//OUT: 40
//OUT: 50
// `count` exists as both a field and a method: the bare access reads the
// field; the call form resolves the method (whose body itself reads the field
// via the no-paren form).
const Jar = struct {
    count: i32,

    fn count(self: Jar) i32 {
        return self.count * 10;
    }
};

pub fn main() void {
    var j: Jar = Jar{ .count = 4 };
    print(j.count);          // 4 — field access
    print(j.count());        // 40 — method call
    j.count = j.count + 1;   // field assignment still targets the field
    print(j.count());        // 50
}
