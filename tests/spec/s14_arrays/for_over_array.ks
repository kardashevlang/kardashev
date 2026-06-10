//SPEC: §29.1 `for` iterates an array's elements in order by value; `, 0..` adds a 0-based usize index
//OUT: 12
//OUT: 15

pub fn main() void {
    var xs: [4]i64 = [4]i64{ 5, 1, 4, 2 };
    var sum: i64 = 0;
    for (xs) |v| {
        sum = sum + v;
    }
    print(sum);           // 5 + 1 + 4 + 2
    // The indexed form: a weighted sum proves the index runs 0,1,2,3 in
    // step with the elements (5*0 + 1*1 + 4*2 + 2*3 = 15).
    var weighted: i64 = 0;
    for (xs, 0..) |v, i| {
        weighted = weighted + v * @as(i64, i);
    }
    print(weighted);
}
