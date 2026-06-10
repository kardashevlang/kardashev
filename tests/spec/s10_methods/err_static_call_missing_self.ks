//SPEC: §10 calling a method through the type name without the `self` argument is E0172
//ERR: E0172
const C = struct {
    n: i32,

    fn get(self: C) i32 {
        return self.n;
    }
};

pub fn main() void {
    print(C.get());   // explicit-self form requires the receiver argument
}
