//SPEC: §23.1 `[]u8` is a first-class field type — slice ops (`print`, `.len`, indexing) work through a struct field
//OUT: Ada
//OUT: 3
//OUT: 65

const Person = struct {
    name: []u8,
    age: i32,
};

pub fn main() void {
    var p: Person = Person{ .name = "Ada", .age = 36 };
    print(p.name);
    print(p.name.len);
    print(p.name[0]);   // 'A' = 65
}
