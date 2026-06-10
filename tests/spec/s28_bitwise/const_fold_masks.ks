//SPEC: §28.3 `const_eval` folds all bitwise/shift operators and `~` — mask-building consts work at top level
//OUT: 255
//OUT: 33
//OUT: 255
//OUT: 32
//OUT: 16
//OUT: 232

const MASK = (1 << 8) - 1;       // the SPEC's own example: 255
const FLAG_A = 1 << 0;
const FLAG_C = 1 << 5;
const COMBO = FLAG_A | FLAG_C;   // 33 — folds across earlier consts
const LOW = ~0 & MASK;           // ~ folds too: -1 & 255 = 255
const WITHOUT_A = COMBO ^ FLAG_A; // 32
const SIXTEENTH = 256 >> 4;      // 16

pub fn main() void {
    print(MASK);
    print(COMBO);
    print(LOW);
    print(WITHOUT_A);
    print(SIXTEENTH);
    var v: i64 = 1000;           // 0b1111101000
    print(v & MASK);             // runtime use of a folded mask: 232
}
