//SPEC: §2 while_stmt with "break" ";" and "continue" ";" — continue skips to the next test, break exits the loop
//OUT: 25
//OUT: 9
pub fn main() void {
    var i: i64 = 0;
    var sum: i64 = 0;
    while (i < 100) {
        i = i + 1;
        if (i % 2 == 0) {
            continue; // skip even numbers entirely
        }
        sum = sum + i;
        if (sum >= 25) {
            break; // 1+3+5+7+9 = 25 stops here, with i == 9
        }
    }
    print(sum);
    print(i);
}
