const std = @import("std");

pub fn build(b: *std.build.Builder) void {
    // Standard target options allows the person running `zig build` to choose
    // what target to build for. Here we do not override the defaults, which
    // means any target is allowed, and the default is native. Other options
    // for restricting supported target set are available.
    const target = b.standardTargetOptions(.{});

    // Standard release options allow the person running `zig build` to select
    // between Debug, ReleaseSafe, ReleaseFast, and ReleaseSmall.
    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = "abt",
        .root_source_file = .{ .path = "src/main.zig" },
        .optimize = optimize,
        .target = target,
    });
    exe.addIncludePath(.{ .path = "lib/regez" });
    exe.linkLibC();
    exe.addCSourceFile(.{ .file = .{ .path = "src/regez.c" }, .flags = &.{
        "-Wall",
        "-Wextra",
        "-Werror",
    } });
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run_cmd.step);

    const exe_tests = b.addTest(.{
        .root_source_file = .{ .path = "src/main.zig" },
        .optimize = optimize,
    });
    exe_tests.addIncludePath(.{ .path = "lib/regez" });
    exe_tests.linkLibC();
    exe_tests.addCSourceFile(.{ .file = .{ .path = "lib/regez.c" }, .flags = &.{
        "-Wall",
        "-Wextra",
        "-Werror",
    } });

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&exe_tests.step);
}
