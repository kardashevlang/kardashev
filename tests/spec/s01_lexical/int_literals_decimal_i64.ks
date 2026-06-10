//SPEC: §1 integer literals are [0-9]+ parsed as DECIMAL into i64 — leading zeros do not mean octal, and the i64 extremes are reachable
//OUT: 17
//OUT: 8223372036854775807
//OUT: -9223372036854775808
pub fn main() void {
    // If literals leaked into C verbatim, 010 would be octal 8 and this 15.
    print(007 + 010);
    // The maximum i64 literal lexes; derive a value from it by arithmetic.
    print(9223372036854775807 - 1000000000000000000);
    // i64::MIN is not writable as a literal; reach it by negation arithmetic.
    print(-9223372036854775807 - 1);
}
