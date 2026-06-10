//SPEC: §9.1 an empty struct declares no fields; `Name{}` constructs it and it passes by value
//OUT: 42
//OUT: 84
const Unit = struct {};

fn consume(u: Unit, k: i32) i32 {
    // The empty value carries no data but still flows by value like any struct.
    return k * 2;
}

pub fn main() void {
    var u: Unit = Unit{};
    var t: i32 = consume(u, 21);
    print(t);                  // 42
    var v: Unit = u;           // empty structs copy too
    print(consume(v, t));      // 84
}
