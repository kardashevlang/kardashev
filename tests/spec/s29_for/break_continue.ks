//SPEC: §29.1 the `for` body is a loop scope — `break` leaves it and `continue` advances to the next element
//OUT: 6
//OUT: 5

pub fn main() void {
    var xs: [6]i64 = [6]i64{ 1, 2, 3, 4, 5, 6 };
    var sum: i64 = 0;
    var visits: i64 = 0;
    for (xs) |v| {
        visits += 1;
        if (v == 5) {
            break;            // element 6 is never visited
        }
        if (v % 2 == 1) {
            continue;         // odd elements are skipped before the sum
        }
        sum += v;
    }
    print(sum);               // 2 + 4 = 6
    print(visits);            // 1,2,3,4,5 were visited; 6 was not
}
