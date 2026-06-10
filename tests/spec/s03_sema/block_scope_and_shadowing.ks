//SPEC: §3 a bare block opens a lexical scope: an inner binding shadows the outer one and dies at `}`
//OUT: 40
//OUT: 2
pub fn main() void {
    var x: i64 = 1;
    {
        var x: i64 = 10; // shadows the outer x inside this block only
        x = x * 4;
        print(x); // 40 — the inner x
    }
    x = x + 1; // the outer x is untouched by the block: 1 + 1
    print(x); // 2
}
