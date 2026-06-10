//SPEC: §37 a variant's explicit `= N` sets its value; a following bare variant auto-increments from it
//OUT: 200
//OUT: 201
//OUT: 204
//OUT: 302
//OUT: 100

const Status = enum {
    Ok = 200,
    Created,          // auto: 201
    NoContent = 204,
    Moved = 301,
    Found,            // auto: 302
};

pub fn main() void {
    print(@intFromEnum(Status.Ok));
    print(@intFromEnum(Status.Created));
    print(@intFromEnum(Status.NoContent));
    print(@intFromEnum(Status.Found));
    // Values survive arithmetic — they are real integers, not ordinals.
    print(@intFromEnum(Status.Moved) - @intFromEnum(Status.Created));
}
