//SPEC: §2 stmt := ... | block — a braced block is itself a statement and may nest
//OUT: 13
//OUT: 20
pub fn main() void {
    var total: i64 = 0;
    {
        var inner: i64 = 6;
        total = total + inner;
    }
    {
        var other: i64 = 7;
        total = total + other;
    }
    print(total);
    {
        {
            total = total + 7;
        }
    }
    print(total);
}
