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
const re = @cImport(@cInclude("regez.h"));
const REGEX_T_ALIGNOF = re.sizeof_regex_t;
const REGEX_T_SIZEOF = re.alignof_regex_t;

const usage =
    \\Usage: abt [options] [--] [gradle command]
    \\
    \\Options:
    \\
    \\  -s, --since-commit             Only select projects changed since given commit in this repo
    \\  -i, --include                  Include projects under given path
    \\  -e, --regexp                   A project is selected if its name matches given pattern
    \\  -v, --invert-match             A project is NOT selected if its name matches given pattern
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

fn nextOrFatal(it: *std.process.ArgIterator, cur: []const u8) []const u8 {
    if (it.next()) |v| {
        return v[0 .. v.len - 1];
    } else {
        fatal("expected parameter after {s}", .{cur});
    }
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
    try options.includes.put(cwd, {});
    debug("Added current dir {s} as one root", .{cwd});
    _ = args.skip(); // skip program path
    while (args.next()) |arg| {
        debug("Arg {s}", .{arg});
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
    while (args.next()) |arg| {
        try options.commands.append(arg);
    }
    debug("parse options: {}", .{options});

    return build(allocator, &options);
}
fn build(allocator: Allocator, options: *Options) !void {
    const vc_root = exec(allocator, &[_][]const u8{
        "git",
        "rev-parse",
        "--show-toplevel",
    }, null) catch |e| blk: {
        warn("Find git root fail: {}", .{e});
        break :blk null;
    };
    if (vc_root) |root| {
        var dir = root[0..mem.indexOfScalar(u8, root, '\n').?];
        debug("Add git root {s} as one root", .{dir});
        try options.includes.put(dir, {});
    } else {
        debug("Not in a git dir", .{});
    }

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
    if (options.since_commit) |commit| {
        if (vc_root) |root| {
            try projects.denyUnchanged(root, commit, options.threshold);
        }
    }
    if (options.scan_impacted_projects) {
        try projects.pickDependencies();
    }

    const states = &[_]Projects.State{ .Picked, .Dependency };
    if (options.commands.items.len > 0) {
        const settings_file = options.settings_file orelse "build.settings.gradle.kts";
        var gradle_cmd = try std.ArrayList([]const u8).initCapacity(allocator, options.commands.items.len + 3);
        try gradle_cmd.append(std.os.getenvZ("GRADLE_CMD") orelse "./gradlew");
        try gradle_cmd.appendSlice(options.commands.items);
        try gradle_cmd.append("-c");
        try gradle_cmd.append(settings_file);
        const command = gradle_cmd.items;
        debug("Gradle command is : {s}", .{command});

        var partitions = projects.partition(states, options.threshold);
        while (true) {
            var i = partitions.next();
            if (i == null) break;

            try write(allocator, &i.?, settings_file);
            if (spawn(allocator, command)) |_| {} else |e| {
                fatal("Execute command failed: {s} {}", .{ command, e });
            }
        }
    } else {
        var i = projects.iterate(states);
        try write(allocator, &i, options.settings_file orelse "settings.gradle.kts");
    }
}

const max_depth_allowed = 3;
const Options = struct {
    since_commit: ?[]const u8 = null,
    includes: StringHashMap(void),
    regexp: ?[]const u8 = null,
    invert_match: ?[]const u8 = null,
    settings_file: ?[]const u8 = null,
    threshold: usize = 1000,
    max_depth: usize = 2,
    scan_impacted_projects: bool = false,
    commands: std.ArrayList([]const u8),
};
const Projects = struct {
    allocator: Allocator,
    entries: StringHashMap(Entry) = undefined,

    const Entry = struct {
        path: []const u8,
        root: []const u8,
        is_build_file_kts: bool,
        state: State,
    };
    const State = enum(u2) {
        Added,
        Picked,
        Denied,
        Dependency,
    };
    pub fn init(allocator: Allocator) Projects {
        return Projects{
            .allocator = allocator,
            .entries = StringHashMap(Entry).init(allocator),
        };
    }

    pub fn scan(self: *@This(), root: []const u8, max_depth: usize) !void {
        debug("Start scanning {s}", .{root});
        var projects = &self.entries;
        var names = [_][]const u8{""} ** (max_depth_allowed * 2);
        var dir_stack: [max_depth_allowed + 1]std.fs.IterableDir = undefined;
        var iter_stack: [max_depth_allowed + 1]std.fs.IterableDir.Iterator = undefined;
        dir_stack[0] = try std.fs.openIterableDirAbsolute(root, .{});
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
                debug("Found {s}", .{name});
                if (f.kind == .File and (mem.eql(u8, name, "build.gradle.kts") or mem.eql(u8, name, "build.gradle"))) {
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
                    if (name_index > 0 and mem.eql(u8, name, "android") or mem.eql(u8, name, "domain")) {
                        names[name_index - 1] = "-";
                    }
                    const p_name = try mem.concat(self.allocator, u8, names[0 .. name_index + 1]);
                    const p = Entry{
                        .path = path,
                        .root = root,
                        .is_build_file_kts = mem.endsWith(u8, name, "kts"),
                        .state = .Added,
                    };
                    info("Found project {s} {any}, added", .{ p_name, p });
                    try projects.put(p_name, p);
                    entry = null;
                } else if (f.kind == .Directory and sp < max_depth and !mem.startsWith(u8, name, ".")) {
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

    pub fn pick(self: *@This(), regexp: []const u8) !void {
        return self.move(regexp, .Added, .Picked);
    }

    pub fn pickAll(self: *@This()) !void {
        var iter = (&self.entries).valueIterator();
        while (iter.next()) |v| {
            if (v.state == .Added) {
                v.state = .Picked;
            }
        }
        debug("Move all .Added to .Picked", .{});
    }

    pub fn pickDependencies(_: *Projects) !void {}

    pub fn deny(self: *@This(), regexp: []const u8) !void {
        return self.move(regexp, .Picked, .Denied);
    }

    pub fn denyUnchanged(self: *@This(), root: []const u8, since_commit: []const u8, max_depth: usize) !void {
        if (exec(self.allocator, &[_][]const u8{
            "git", "diff", "--name-only", "--merge-base", since_commit,
        }, root)) |changes| {
            var dirs = StringHashMap(void).init(self.allocator);
            var lines = mem.tokenize(u8, changes, "\n");
            while (lines.next()) |line| {
                var i = @as(usize, 0);
                var depth = @as(usize, 0);
                while (i < line.len and depth < max_depth) : (depth += 1) {
                    i = mem.indexOfScalarPos(u8, line, i, std.fs.path.sep) orelse line.len;
                    try dirs.put(line[0..i], {});
                }
            }

            var iter = (&self.entries).valueIterator();
            while (iter.next()) |v| {
                if (v.state == .Picked and !dirs.contains(v.path)) {
                    v.state = .Denied;
                }
            }
        } else |e| {
            fatal("Can't get git diff, {}", .{e});
        }
    }

    const Iterator = struct {
        states: []const State,
        internal_iter: StringHashMap(Projects.Entry).Iterator,
        max_count: usize = std.math.maxInt(usize),
        i: usize = 0,

        const Entry = struct {
            name: []const u8,
            path: []const u8,
            root: []const u8,
        };
        fn next(self: *@This()) ?Iterator.Entry {
            defer self.i += 1;

            if (self.i < self.max_count) {
                while (self.internal_iter.next()) |kv| {
                    if (mem.indexOfScalar(State, self.states, kv.value_ptr.state) == null)
                        continue;
                    return Iterator.Entry{
                        .name = kv.key_ptr.*,
                        .path = kv.value_ptr.path,
                        .root = kv.value_ptr.root,
                    };
                }
            }
            return null;
        }
    };

    pub fn iterate(self: *@This(), states: []const State) Iterator {
        return Iterator{
            .states = states,
            .internal_iter = self.entries.iterator(),
        };
    }

    const PartitionIterator = struct {
        states: []const State,
        internal_iter: StringHashMap(Entry).Iterator,
        threshold: usize = std.math.maxInt(usize),

        fn next(self: *@This()) ?Iterator {
            if (self.internal_iter.index % self.threshold == 0) {
                return Iterator{
                    .states = self.states,
                    .internal_iter = self.internal_iter,
                    .max_count = self.threshold,
                };
            }
            return null;
        }
    };

    pub fn partition(self: *@This(), states: []const State, threshold: usize) PartitionIterator {
        return PartitionIterator{
            .states = states,
            .internal_iter = self.entries.iterator(),
            .threshold = threshold,
        };
    }

    fn move(self: *@This(), pattern: []const u8, from: State, to: State) !void {
        var slice = try self.allocator.alignedAlloc(u8, REGEX_T_ALIGNOF, REGEX_T_SIZEOF);
        const regex = @ptrCast(*re.regex_t, slice.ptr);
        defer self.allocator.free(@ptrCast([*]u8, regex)[0..REGEX_T_SIZEOF]);

        if (re.regcomp(regex, @ptrCast([*c]const u8, pattern), 0) != 0) {
            fatal("Invalid regex: {s}", .{pattern});
        }
        defer re.regfree(regex);

        var iter = (&self.entries).iterator();
        while (iter.next()) |kv| {
            if (kv.value_ptr.state == from and re.isMatch(regex, @ptrCast([*c]const u8, kv.key_ptr.*))) {
                debug("Move {s} from {} to {}", .{ kv.key_ptr.*, from, to });
                kv.value_ptr.state = to;
            }
        }
    }
};

fn write(allocator: Allocator, projects: *Projects.Iterator, settings_file: []const u8) !void {
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
    while (projects.next()) |p| {
        debug("Add project {} to {s}", .{ p, settings_file });
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
        , .{ p.name, p.name, relative.value_ptr.*, p.path });
        defer allocator.free(text);

        _ = try file.writeAll(text);
    }
}

fn exec(allocator: Allocator, cmd: []const []const u8, cwd: ?[]const u8) ![]const u8 {
    debug("Execute external command: {s}", .{cmd});
    const result = try std.ChildProcess.exec(.{
        .allocator = allocator,
        .argv = cmd,
        .cwd = cwd,
    });

    if (result.stderr.len > 0) {
        std.log.err("{s}", .{result.stderr});
    }
    debug("Command finished with: {any}", .{result});
    return result.stdout;
}

fn spawn(allocator: Allocator, cmd: [][]const u8) !std.ChildProcess.Term {
    var child = std.ChildProcess.init(cmd, allocator);
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;

    return child.spawnAndWait();
}

fn fatal(comptime format: []const u8, args: anytype) noreturn {
    std.log.err(format, args);
    process.exit(1);
}
