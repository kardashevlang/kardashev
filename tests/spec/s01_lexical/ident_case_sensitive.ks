//SPEC: §1 identifiers are case-sensitive — foo, Foo and FOO name three distinct bindings
//OUT: 279
//OUT: 85
const foo: i64 = 1;
const Foo: i64 = 20;
const FOO: i64 = 300;

pub fn main() void {
    print(FOO - Foo - foo);
    var value: i64 = 5;
    var Value: i64 = 8;
    print(Value * 10 + value);
}
