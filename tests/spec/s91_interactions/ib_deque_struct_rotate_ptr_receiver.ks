//SPEC: std Deque(T)×§30 struct elements ride the ring intact — popped copies mutated via pointer-receiver methods and re-pushed survive growth and wraparound
//OUT: 33
//OUT: 44
//OUT: 55
//OUT: 12
//OUT: 24

@import("std");

const Job = struct {
    id: i64,
    runs: i64,
    fn work(self: *Job, amount: i64) void {
        self.runs += amount;
    }
};

const Q = Deque(Job);

pub fn main() void {
    var a: Allocator = c_allocator();
    var q: Q = Q.init(a);

    // 5 pushes on an initial capacity of 4 -> the ring grows once.
    var n: i64 = 1;
    while (n <= 5) : (n += 1) {
        q.push_back(a, Job{ .id = n, .runs = 0 });
    }

    // 7 rotations: pop the front, mutate via the pointer receiver, push it
    // back — the head wraps around the ring. work(id) adds id per rotation;
    // jobs 1 and 2 rotate twice (runs 2 and 4), jobs 3, 4, 5 once (runs = id).
    var r: i64 = 0;
    while (r < 7) : (r += 1) {
        var j: Job = q.pop_front();
        j.work(j.id);
        q.push_back(a, j);
    }

    // Remaining order: 3, 4, 5, 1, 2 with runs 3, 4, 5, 2, 4.
    while (q.len() > 0) {
        var j: Job = q.pop_front();
        print(j.id * 10 + j.runs);
    }
    q.deinit(a);
}
