//SPEC: §15.2 a slice is a non-owning view — writes through `s[i] = e` hit the array and array writes show through the slice
//OUT: 22
//OUT: 42

pub fn main() void {
    var data: [5]i64 = [5]i64{ 10, 20, 30, 40, 50 };
    var s: []i64 = data[0..5];

    s[1] = s[1] + 2;       // through the slice ...
    print(data[1]);        // ... observed in the array: 22

    data[3] = data[3] + 2; // into the array ...
    print(s[3]);           // ... observed through the slice: 42
}
