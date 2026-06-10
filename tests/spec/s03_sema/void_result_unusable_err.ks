//SPEC: §3 a `void` call produces no value — it cannot initialize a binding
//ERR: E0110
fn ping() void {
    print(1);
}
pub fn main() void {
    var x: i64 = ping();
    print(x);
}
