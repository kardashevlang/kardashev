//SPEC: §21.2 `break`/`continue` are normal exits — loop-body defers flush, errdefers do not
//OUT: 24

// Per iteration the body registers: defer acc+=1, errdefer acc+=100.
//   i=0: acc+=10 -> 10, fall-through flush -> 11
//   i=1: continue flush               -> 12
//   i=2: acc+=10 -> 22, fall-through  -> 23
//   i=3: break flush                  -> 24 (loop over)
// Any errdefer firing would add 100 and be unmistakable.
fn walk() i64 {
    var i: i64 = 0;
    var acc: i64 = 0;
    while (i < 5) : (i = i + 1) {
        defer acc = acc + 1;
        errdefer acc = acc + 100;
        if (i == 1) {
            continue;
        }
        if (i == 3) {
            break;
        }
        acc = acc + 10;
    }
    return acc;
}

pub fn main() void {
    print(walk());
}
