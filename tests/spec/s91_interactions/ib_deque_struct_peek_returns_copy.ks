//SPEC: std Deque(T)×§9 `front()` yields a struct COPY — mutating the peeked value (even via a pointer-receiver method) leaves the deque's element untouched
//OUT: 1
//OUT: 0
//OUT: 20
//OUT: 10

@import("std");

const Job = struct {
    id: i64,
    runs: i64,
    fn work(self: *Job) void {
        self.runs += 1;
    }
};

const Q = Deque(Job);

pub fn main() void {
    var a: Allocator = c_allocator();
    var q: Q = Q.init(a);
    q.push_back(a, Job{ .id = 1, .runs = 0 });
    q.push_front(a, Job{ .id = 2, .runs = 0 });  // order now: 2, 1

    var f: Job = q.front();   // value copy of job 2
    f.work();                 // auto-ref of the LOCAL copy
    print(f.runs);            // 1 — the copy changed

    var g: Job = q.front();
    print(g.runs);            // 0 — the element in the ring did not

    // Drain: ids 2 then 1, both with runs still 0.
    var x: Job = q.pop_front();
    print(x.id * 10 + x.runs);  // 20
    var y: Job = q.pop_front();
    print(y.id * 10 + y.runs);  // 10
    q.deinit(a);
}
