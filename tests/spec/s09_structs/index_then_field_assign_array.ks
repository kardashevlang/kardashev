//SPEC: §9.4+§14.1 a field assignment whose chain passes through an array index (`arr[i].f = e`) writes the element's field
//OUT: 30
//OUT: 4
const P = struct {
    x: i32,
    y: i32,
};

pub fn main() void {
    var arr: [2]P = [2]P{ P{ .x = 1, .y = 2 }, P{ .x = 3, .y = 4 } };
    arr[1].x = 30;
    print(arr[1].x);   // 30
    print(arr[1].y);   // 4
}
