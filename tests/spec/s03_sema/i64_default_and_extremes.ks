//SPEC: §3 integer literals default to `i64` with no expectation; i64 spans -2^63 .. 2^63-1
//OUT: 8589934592
//OUT: 9223372036854775807
//OUT: -9223372036854775808
//OUT: -1
pub fn main() void {
    // 2^31 * 4 is only computable in a 64-bit type: a narrower default
    // could not even represent the operand, let alone the product.
    print(2147483648 * 4);

    // Reach i64 max by arithmetic (never by overflow).
    var max: i64 = 9223372036854775806;
    max = max + 1;
    print(max);

    // i64 min is one below the largest literal; build it by subtraction.
    var min: i64 = -9223372036854775807 - 1;
    print(min);

    // min + max = -1 exactly when both extremes are 64-bit two's complement.
    print(min + max);
}
