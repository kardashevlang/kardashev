//SPEC: §38 `== != < <= > >=` on two `f64` yield `bool`
//OUT: 31

pub fn main() void {
    var score: i64 = 0;
    // Dyadic values, so == on a computed sum is reliable.
    if (0.5 + 0.25 == 0.75) {
        score += 1;
    }
    if (1.5 != 1.25) {
        score += 2;
    }
    if (1.25 < 1.5) {
        score += 4;
    }
    if (1.5 <= 1.5) {
        score += 8;
    }
    if (2.5 > 2.25) {
        score += 16;
    }
    if (2.25 >= 2.5) {
        score += 32;     // false — must NOT fire
    }
    print(score);        // 1+2+4+8+16 = 31
}
