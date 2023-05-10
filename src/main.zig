const std = @import("std");
const builtin = @import("builtin");
const assert = std.debug.assert;
const io = std.io;
const fs = std.fs;
const mem = std.mem;
const process = std.process;
const Allocator = mem.Allocator;
const ArrayList = std.ArrayList;
const StringHashMap = std.StringHashMap;
const warn = std.log.warn;
const debug = std.log.debug;
const info = std.log.info;

const usage =
    \\Usage: abt [options] [--] [gradle command]
    \\
    \\Options:
    \\
    \\  -s, --since-commit             Only select projects changed since given commit in this repo
    \\  -i, --include                  Include projects under given path
    \\  -e, --regexp                   A project is selected if its name matches given pattern
    \\  -v, --invert-match             A project is NOT selected if its name matches given pattern
    \\  -f, --filter                   A project is selected if the given shell command pass in its directory
    \\  -c, --settings-file            The gradle settings file will be generated and used
    \\  --threshold                    The max number of project can run at one time, projects more than it will be sepearted into many run
    \\  --max-depth                    Descend at most n directory levels
    \\  --scan-impacted-projects       Add projects impacted by selected projects too
    \\  -h, --help                     Print command-specific usage
    \\
    \\Environments:
    \\
    \\ GRADLE_CMD                      The gradel command to run for building, you can give args here too
    \\
;

fn nextOrFatal(it: *std.process.ArgIterator, cur: []const u8) [:0]const u8 {
    return it.next() orelse fatal("expected parameter after {s}", .{cur});
}
pub fn main() !void {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    var args = try process.argsWithAllocator(allocator);

    var options = Options{
        .includes = StringHashMap(void).init(allocator),
        .commands = std.ArrayList([]const u8).init(allocator),
    };
    const cwd = try std.fs.cwd().realpathAlloc(allocator, ".");
    _ = args.skip(); // skip program path
    while (args.next()) |arg| {
        if (mem.eql(u8, arg, "-h") or mem.eql(u8, arg, "--help")) {
            return io.getStdOut().writeAll(usage);
        }
        if (mem.eql(u8, arg, "--")) {
            _ = args.next();
            break;
        }

        if (mem.eql(u8, arg, "-s") or mem.eql(u8, arg, "--since-commit")) {
            options.since_commit = nextOrFatal(&args, arg);
        } else if (mem.eql(u8, arg, "-i") or mem.eql(u8, arg, "--include")) {
            try options.includes.put(try std.fs.path.resolve(allocator, &[_][]const u8{ cwd, nextOrFatal(&args, arg) }), {});
        } else if (mem.eql(u8, arg, "-e") or mem.eql(u8, arg, "--regexp")) {
            options.regexp = nextOrFatal(&args, arg);
        } else if (mem.eql(u8, arg, "-v") or mem.eql(u8, arg, "--invert-match")) {
            options.invert_match = nextOrFatal(&args, arg);
        } else if (mem.eql(u8, arg, "-f") or mem.eql(u8, arg, "--filter")) {
            options.filter = nextOrFatal(&args, arg);
        } else if (mem.eql(u8, arg, "-c") or mem.eql(u8, arg, "--settings-file")) {
            options.settings_file = nextOrFatal(&args, arg);
        } else if (mem.eql(u8, arg, "--threshold")) {
            options.threshold = try std.fmt.parseInt(usize, nextOrFatal(&args, arg), 10);
        } else if (mem.eql(u8, arg, "--max-depth")) {
            const max_depth = try std.fmt.parseInt(usize, nextOrFatal(&args, arg), 10);
            std.debug.assert(max_depth > 1 and max_depth <= max_depth_allowed);
            options.max_depth = max_depth;
        } else if (mem.eql(u8, arg, "--scan-impacted-projects")) {
            options.scan_impacted_projects = true;
        } else {
            try options.commands.append(arg);
            break;
        }
    }
    try options.includes.put(cwd, {});
    debug("Added current dir {s} as one root", .{cwd});
    while (args.next()) |arg| {
        try options.commands.append(arg);
    }
    debug("parse options: {}", .{options});

    return build(allocator, &options);
}
fn build(allocator: Allocator, options: *Options) !void {
    const output = exec(allocator, &[_][]const u8{
        "git",
        "rev-parse",
        "--show-toplevel",
    }, null) catch |e| blk: {
        warn("Find git root fail: {}", .{e});
        break :blk null;
    };
    const vc_root = if (output) |root| blk: {
        var dir = mem.trimRight(u8, root, "\n");
        debug("Add git root {s} as one root", .{dir});
        try options.includes.put(dir, {});
        break :blk dir;
    } else blk: {
        debug("Not in a git dir", .{});
        break :blk null;
    };

    var projects = Projects.init(allocator);
    var iter = options.includes.keyIterator();
    while (iter.next()) |root| {
        try projects.scan(root.*, options.max_depth);
    }
    if (options.regexp) |pattern| {
        try projects.pick(pattern);
    } else {
        try projects.pickAll();
    }
    if (options.invert_match) |pattern| {
        try projects.deny(pattern);
    }
    if (options.filter) |pattern| {
        try projects.filter(pattern);
    }
    if (options.since_commit) |commit| {
        if (vc_root) |root| {
            try projects.denyUnchanged(root, commit, options.threshold);
        }
    }
    if (options.scan_impacted_projects) {
        try projects.pickDependencies();
    }

    const settings_file = options.settings_file orelse if (options.commands.items.len > 0) "build.settings.gradle.kts" else "settings.gradle.kts";
    var partitions = projects.entries[@enumToInt(Projects.State.Picked)].items;
    if (partitions.len > 0 and options.commands.items.len > 0) {
        var gradle_cmd = try std.ArrayList([]const u8).initCapacity(allocator, options.commands.items.len + 3);
        try gradle_cmd.append(std.os.getenvZ("GRADLE_CMD") orelse "./gradlew");
        try gradle_cmd.appendSlice(options.commands.items);
        try gradle_cmd.append("-c");
        try gradle_cmd.append(settings_file);
        const command = gradle_cmd.items;
        debug("Gradle command is : {s}", .{command});

        var i = @as(usize, 0);
        while (i < partitions.len) {
            const end = @min(partitions.len, i + options.threshold);
            try write(allocator, partitions[i..end], settings_file);
            i = end;
            info("Execute {s}", .{command});
            if (spawn(allocator, command, null)) |term| {
                if (term.Exited != 0) {
                    fatal("Execute command failed: {s} {}", .{ command, term.Exited });
                }
            } else |e| {
                fatal("Execute command failed: {s} {}", .{ command, e });
            }
        }
    } else {
        try write(allocator, partitions, settings_file);
    }
}

const max_depth_allowed = 3;
const Options = struct {
    since_commit: ?[]const u8 = null,
    includes: StringHashMap(void),
    regexp: ?[:0]const u8 = null,
    invert_match: ?[:0]const u8 = null,
    filter: ?[:0]const u8 = null,
    settings_file: ?[]const u8 = null,
    threshold: usize = 1000,
    max_depth: usize = 2,
    scan_impacted_projects: bool = false,
    commands: std.ArrayList([]const u8),
};
const Projects = struct {
    allocator: Allocator,
    entries: [@enumToInt(State.Denied) + 1]ArrayList(Entry) = undefined,

    const Entry = struct {
        name: [:0]const u8,
        path: []const u8,
        root: []const u8,
        is_build_file_kts: bool,
    };
    const State = enum(u2) {
        Added,
        Picked,
        Denied,
    };

    pub fn init(allocator: Allocator) Projects {
        var self = Projects{
            .allocator = allocator,
        };
        for (self.entries) |*p| {
            p.* = ArrayList(Entry).init(allocator);
        }
        return self;
    }

    pub fn scan(self: *@This(), root: []const u8, max_depth: usize) !void {
        debug("Start scanning {s}", .{root});
        var projects = &self.entries[@enumToInt(State.Added)];
        var names = [_][]const u8{""} ** (max_depth_allowed * 2);
        var dir_stack: [max_depth_allowed + 1]std.fs.IterableDir = undefined;
        var iter_stack: [max_depth_allowed + 1]std.fs.IterableDir.Iterator = undefined;
        dir_stack[0] = std.fs.openIterableDirAbsolute(root, .{}) catch fatal("Can't open directory: {s}", .{root});
        iter_stack[0] = (&dir_stack[0]).iterate();
        var sp = @as(usize, 0);
        debug("Enter {s}", .{root});
        while (sp >= 0) {
            var entry = (&iter_stack[sp]).next() catch |e| blk: {
                warn("Failed to iterate dir {}", .{e});
                break :blk null;
            };
            if (entry) |f| {
                const name = f.name;
                if (sp > 0 and f.kind == .File and (mem.eql(u8, name, "build.gradle.kts") or mem.eql(u8, name, "build.gradle"))) {
                    const name_index = (sp - 1) * 2;
                    var i = @as(usize, 1);
                    while (i < name_index) : (i += 2) {
                        names[i] = std.fs.path.sep_str;
                    }
                    const path = try mem.concat(self.allocator, u8, names[0 .. name_index + 1]);
                    i = 1;
                    while (i < name_index) : (i += 2) {
                        names[i] = ":";
                    }
                    if (name_index > 0 and mem.eql(u8, names[name_index], "android") or mem.eql(u8, names[name_index], "domain")) {
                        names[name_index - 1] = "-";
                    }
                    const p_name = try mem.concatWithSentinel(self.allocator, u8, names[0 .. name_index + 1], 0);
                    const p = Entry{
                        .name = p_name,
                        .path = path,
                        .root = root,
                        .is_build_file_kts = mem.endsWith(u8, name, "kts"),
                    };
                    debug("Found project {s} at {s}/{s}, added", .{ p_name, root, path });
                    try projects.append(p);
                    entry = null;
                } else if (f.kind == .Directory and sp < max_depth and !mem.startsWith(u8, name, ".")) {
                    debug("Found {s}", .{name});
                    names[sp * 2] = name;
                    const depth = sp + 1;
                    debug("Enter level {} dir: {s}", .{ depth, name });
                    dir_stack[depth] = try (&dir_stack[sp]).dir.openIterableDir(name, .{});
                    sp = depth;
                    iter_stack[sp] = (&dir_stack[sp]).iterate();
                }
            }

            if (entry == null) {
                const cur = sp;
                defer _ = &dir_stack[cur].close();
                if (sp == 0) {
                    debug("Leave {s}", .{root});
                    break;
                }
                sp -= 1;
                debug("Back to {s}", .{names[sp * 2]});
            }
        }
        debug("Finish scanning", .{});
    }

    pub fn pick(self: *@This(), regexp: [:0]const u8) !void {
        return self.move(regexp, .Added, .Picked);
    }

    pub fn pickAll(self: *@This()) !void {
        info("Move all .Added to .Picked", .{});
        try self.entries[@enumToInt(State.Picked)].appendSlice(self.entries[@enumToInt(State.Added)].toOwnedSlice());
    }

    pub fn pickDependencies(_: *Projects) !void {}

    pub fn deny(self: *@This(), regexp: [:0]const u8) !void {
        return self.move(regexp, .Picked, .Denied);
    }

    pub fn filter(self: *@This(), script: []const u8) !void {
        info("Move projects based on filter {s}", .{script});
        var from_list = &self.entries[@enumToInt(State.Picked)];
        var to_list = &self.entries[@enumToInt(State.Denied)];
        var i = @as(usize, 0);
        while (i < from_list.items.len) {
            const path = from_list.items[i].path;
            debug("checking {s}", .{path});
            if (spawn(self.allocator, &[_][]const u8{
                "sh", "-c", script,
            }, try std.fs.path.resolve(self.allocator, &[_][]const u8{ from_list.items[i].root, path }))) |term| {
                if (term.Exited != 0) {
                    try to_list.append(from_list.swapRemove(i));
                } else {
                    info("Keep {s} in .Picked", .{path});
                    i += 1;
                }
            } else |e| {
                fatal("Run filter {s} under {s} failed: {}", .{ script, path, e });
            }
        }
    }

    pub fn denyUnchanged(self: *@This(), root: []const u8, since_commit: []const u8, max_depth: usize) !void {
        info("Move projects based on changes since commit {s}", .{since_commit});
        if (exec(self.allocator, &[_][]const u8{
            "git", "diff", "--name-only", since_commit,
        }, root)) |changes| {
            var dirs = StringHashMap(void).init(self.allocator);
            try cacheDirs(changes, max_depth, &dirs);
            try cacheDirs(exec(self.allocator, &[_][]const u8{
                "git", "ls-files", "-o", "--exclude-standard", "--modified",
            }, root) catch "", max_depth, &dirs);

            var from_list = &self.entries[@enumToInt(State.Picked)];
            var to_list = &self.entries[@enumToInt(State.Denied)];
            var i = @as(usize, 0);
            while (i < from_list.items.len) {
                debug("checking {s}", .{from_list.items[i].path});
                if (!dirs.contains(from_list.items[i].path)) {
                    info("Move {s} from .Picked to .Denied", .{from_list.items[i].path});
                    try to_list.append(from_list.swapRemove(i));
                } else {
                    i += 1;
                }
            }
        } else |e| {
            fatal("Can't get git diff, {}", .{e});
        }
    }
    inline fn cacheDirs(files: []const u8, max_depth: usize, cache: *StringHashMap(void)) !void {
        var lines = mem.tokenize(u8, files, "\n");
        while (lines.next()) |line| {
            debug("File changed: {s}", .{line});
            var i = @as(usize, 0);
            var depth = @as(usize, 0);
            while (i < line.len and depth < max_depth) : (depth += 1) {
                const j = mem.indexOfScalarPos(u8, line, i, std.fs.path.sep) orelse line.len;
                try cache.put(line[0..j], {});
                debug("add change dir: {s}", .{line[0..j]});
                i = j + 1;
            }
        }
    }

    fn move(self: *@This(), pattern: [:0]const u8, from: State, to: State) !void {
        info("Move projects state based on the regexp {s}", .{pattern});
        var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
        defer arena.deinit();
        const allocator = arena.allocator();
        const re = @cImport(@cInclude("regez.h"));
        var slice = try allocator.alignedAlloc(u8, re.alignof_regex_t, re.sizeof_regex_t);
        const regex = @ptrCast(*re.regex_t, slice.ptr);
        var buf = try allocator.alloc(u8, 512);
        const buf_ptr = @ptrCast([*c]u8, buf.ptr);
        mem.copy(u8, buf[0..pattern.len], pattern);
        buf[pattern.len] = 0;
        if (re.regcomp(regex, buf_ptr, re.REG_EXTENDED) != 0) {
            fatal("Invalid regex '{s}'", .{pattern});
        }
        var from_list = &self.entries[@enumToInt(from)];
        var to_list = &self.entries[@enumToInt(to)];
        var i = @as(usize, 0);
        while (i < from_list.items.len) {
            const name = from_list.items[i].name;
            mem.copy(u8, buf[0..name.len], name);
            buf[name.len] = 0;
            const ret = re.isMatch(regex, buf_ptr);
            if (ret == 0) {
                info("Move {s} from {} to {}", .{ name, from, to });
                try to_list.append(from_list.swapRemove(i));
            } else {
                debug("Checking project {s}: return {}", .{ buf_ptr, ret });
                i += 1;
            }
        }
    }
};

fn write(allocator: Allocator, projects: []Projects.Entry, settings_file: []const u8) !void {
    const cwd = std.fs.cwd();
    const dir = if (std.fs.path.dirname(settings_file)) |dir| try std.fs.cwd().openDir(dir, .{}) else cwd;
    const file = dir.createFile(settings_file, .{
        .truncate = true,
    }) catch |ex| {
        fatal("Can create file {s} {}ex", .{ settings_file, ex });
    };
    defer file.close();
    _ = try file.writeAll("// this is auto generated, please don't edit.\n// You can add logic in settings.pre.gradle.kts instead.\n// Ue `abt -v open` can regenerate this file.\n");
    if (dir.openFile("settings.pre.gradle.kts", .{})) |pre| {
        defer pre.close();
        var buf: [2048]u8 = undefined;
        while (pre.readAll(&buf)) |count| {
            _ = try file.writeAll(buf[0..count]);

            if (count < buf.len) {
                break;
            }
        } else |e| {
            warn("write to settings file failed: {}", .{e});
        }
    } else |e| {
        warn("read settings.pre.gradle.kts file failed: {}", .{e});
    }

    debug("Start writing projects into {s}", .{settings_file});
    var relative_paths = StringHashMap([]const u8).init(allocator);
    const dir_path = try dir.realpathAlloc(allocator, ".");
    for (projects) |p| {
        info("Add project {s} to {s}", .{ p.name, settings_file });
        const relative = try relative_paths.getOrPut(p.root);
        if (!relative.found_existing) {
            relative.value_ptr.* = try std.fs.path.relative(allocator, dir_path, p.root);
            if (relative.value_ptr.*.len == 0) {
                relative.value_ptr.* = ".";
            }
        }
        const text = try std.fmt.allocPrint(allocator,
            \\include(":{s}")
            \\project(":{s}").projectDir = file("{s}/{s}")
            \\
            \\
        , .{ p.name, p.name, relative.value_ptr.*, p.path });
        defer allocator.free(text);

        _ = try file.writeAll(text);
    }
}

fn exec(allocator: Allocator, cmd: []const []const u8, cwd: ?[]const u8) ![]const u8 {
    info("Execute external command: {s} in {s}", .{ cmd, cwd orelse "." });
    const result = try std.ChildProcess.exec(.{
        .allocator = allocator,
        .argv = cmd,
        .cwd = cwd,
        .max_output_bytes = 5 * 1024 * 1024,
    });

    if (result.stderr.len > 0) {
        std.log.err("{s}", .{result.stderr});
    }
    debug("Command finished with: {any}", .{result});
    return result.stdout;
}

fn spawn(allocator: Allocator, cmd: [][]const u8, cwd: ?[]const u8) !std.ChildProcess.Term {
    var child = std.ChildProcess.init(cmd, allocator);
    if (cwd) |dir| {
        child.cwd = dir;
    }
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;

    return child.spawnAndWait();
}

fn fatal(comptime format: []const u8, args: anytype) noreturn {
    std.log.err(format, args);
    process.exit(1);
}

test "test regex common patterns" {
    const pattern = "foo|bar";

    var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
    defer arena.deinit();
    const allocator = arena.allocator();
    const re = @cImport(@cInclude("regez.h"));
    var slice = try allocator.alignedAlloc(u8, re.alignof_regex_t, re.sizeof_regex_t);
    const regex = @ptrCast(*re.regex_t, slice.ptr);
    var buf = try allocator.alloc(u8, 512);
    const buf_ptr = @ptrCast([*c]u8, buf.ptr);
    mem.copy(u8, buf[0..pattern.len], pattern);
    buf[pattern.len] = 0;
    if (re.regcomp(regex, buf_ptr, re.REG_EXTENDED) != 0) {
        fatal("Invalid regex '{s}'", .{pattern});
    }

    mem.copy(u8, buf[0..5], "food");
    std.debug.assert(re.isMatch(regex, buf_ptr) == 0);

    mem.copy(u8, buf[0..6], "abart");
    std.debug.assert(re.isMatch(regex, buf_ptr) == 0);

    mem.copy(u8, buf[0..5], "foba");
    std.debug.assert(re.isMatch(regex, buf_ptr) == 1);
}
