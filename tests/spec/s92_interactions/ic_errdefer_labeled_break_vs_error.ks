//SPEC: §21.2 x §40 an errdefer inside a labeled loop flushes on the try-propagation edge but NOT on a labeled break
//OUT: 3
//OUT: 77
//OUT: -1

fn step(i: i64) !i64 {
    if (i == 3) { return error.Stop; }
    return i;
}

fn scan(limit: i64) !i64 {
    var acc: i64 = 0;
    outer: while (true) {
        errdefer print(77);
        var i: i64 = 0;
        while (i < limit) : (i += 1) {
            var v: i64 = try step(i);
            acc += v;
        }
        break :outer;   // a success edge: the errdefer must NOT run
    }
    return acc;
}

pub fn main() void {
    print(scan(3) catch (0 - 1));   // 0+1+2 = 3, no 77
    print(scan(4) catch (0 - 1));   // i == 3 propagates: 77 then the fallback
}
