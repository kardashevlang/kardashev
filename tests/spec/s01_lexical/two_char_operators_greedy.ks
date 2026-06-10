//SPEC: §1 two-char operators (== != <= >= << >>) win over their one-char prefixes
//OUT: 1111
//OUT: 4
//OUT: 4
pub fn main() void {
    // Each comparison would be a parse error if its two chars lexed apart.
    var count: i64 = 0;
    if (2 <= 2) {
        count = count + 1;
    }
    if (3 >= 3) {
        count = count + 10;
    }
    if (1 != 2) {
        count = count + 100;
    }
    if (4 == 4) {
        count = count + 1000;
    }
    print(count);
    // `<<`/`>>` are single tokens, not `<` `<` / `>` `>`.
    print(1 << 2);
    print(9 >> 1);
}
