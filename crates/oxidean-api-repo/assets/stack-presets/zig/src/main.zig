const std = @import("std");

pub fn greet(name: []const u8, buf: []u8) ![]const u8 {
    return try std.fmt.bufPrint(buf, "Hello, {s}!", .{name});
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var args = try std.process.argsWithAllocator(allocator);
    defer args.deinit();
    _ = args.skip();
    const name = args.next() orelse "world";

    var buf: [128]u8 = undefined;
    const msg = try greet(name, &buf);
    try std.io.getStdOut().writer().print("{s}\n", .{msg});
}

test "greet" {
    var buf: [64]u8 = undefined;
    const msg = try greet("Oxidean", &buf);
    try std.testing.expectEqualStrings("Hello, Oxidean!", msg);
}
