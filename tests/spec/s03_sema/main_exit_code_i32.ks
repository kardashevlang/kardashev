//SPEC: §3 `main` may return `i32`; the returned value is the process exit code
//EXIT: 7
fn is_prime(n: i64) bool {
    if (n < 2) {
        return false;
    }
    var d: i64 = 2;
    while (d * d <= n) : (d = d + 1) {
        if (n % d == 0) {
            return false;
        }
    }
    return true;
}
pub fn main() i32 {
    // Primes below 18: 2 3 5 7 11 13 17 — seven of them.
    var count: i64 = 0;
    var n: i64 = 2;
    while (n < 18) : (n = n + 1) {
        if (is_prime(n)) {
            count = count + 1;
        }
    }
    return @as(i32, count);
}
