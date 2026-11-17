let meta = #{
    name: "simplify-backend-crates",
    description: "按 crate 精简后端内部实现，保持业务行为与对外 API，独立复核并执行定向门禁",
    when_to_use: "清理 backend/crates 内的过度防御、过度抽象、死代码、无意义包装和复制粘贴",
    phases: [
        #{ title: "清点", detail: "动态发现 workspace 成员、确认修改基线与执行预算" },
        #{ title: "精简", detail: "每个 crate 一个执行者，按互不重叠的目录分批并行" },
        #{ title: "复核", detail: "逐批独立检查等价性，最多修正并复核一次" },
        #{ title: "验证", detail: "在最终代码上串行执行受影响 crate 的质量门禁，编译目录只放外置盘" },
        #{ title: "报告", detail: "汇总实际改动、等价性依据、跳过项与验证限制" },
    ],
};

// 默认处理 backend/crates 下全部活动成员；crates 可指定包名数组。
// parallelism 默认 4，允许 1..4；recovery_file 可导入首批报告校验失败的现场。
// 所有执行者共享当前工作区，只写各自 crate。
// 只创建本地修改与运行报告；提交、推送、分支操作和外部服务操作均需另行授权。
// 独立编译的产物目录只允许已挂载外置盘 /Volumes/Kingston/rust-build-target。

let strings = #{ "type": "array", "items": #{ "type": "string" } };
let crate_schema = #{
    "type": "object", "required": ["name", "path"],
    "properties": #{ "name": #{ "type": "string" }, "path": #{ "type": "string" } },
};
let inventory_schema = #{
    "type": "object", "required": ["head", "repository_root", "inventory_ok", "backend_clean", "outside_status", "crates", "evidence"],
    "properties": #{
        "head": #{ "type": "string" }, "repository_root": #{ "type": "string" },
        "inventory_ok": #{ "type": "boolean" }, "backend_clean": #{ "type": "boolean" },
        "outside_status": #{ "type": "string" }, "evidence": #{ "type": "string" },
        "crates": #{ "type": "array", "items": crate_schema },
    },
};
let change_schema = #{
    "type": "object", "required": ["category", "files", "summary", "equivalence"],
    "properties": #{
        "category": #{ "type": "string", "enum": ["过度防御", "过度抽象", "死代码", "无意义包装", "重复逻辑"] },
        "files": strings, "summary": #{ "type": "string" }, "equivalence": #{ "type": "string" },
    },
};
let implementation_schema = #{
    "type": "object", "required": ["crate_name", "status", "inspected_files", "files_changed", "changes", "deferred", "evidence", "compiled", "compile_target_dir"],
    "properties": #{
        "crate_name": #{ "type": "string" },
        "status": #{ "type": "string", "enum": ["changed", "unchanged", "blocked"] },
        "inspected_files": strings, "files_changed": strings,
        "changes": #{ "type": "array", "items": change_schema },
        "deferred": strings, "evidence": #{ "type": "string" },
        "compiled": #{ "type": "boolean" }, "compile_target_dir": #{ "type": "string" },
    },
};
let snapshot_schema = #{
    "type": "object", "required": ["head", "status", "outside_status", "index_clean", "backend_untracked", "fingerprint"],
    "properties": #{
        "head": #{ "type": "string" }, "status": #{ "type": "string" },
        "outside_status": #{ "type": "string" }, "index_clean": #{ "type": "boolean" },
        "backend_untracked": strings, "fingerprint": #{ "type": "string" },
    },
};
let recovery_schema = #{
    "type": "object", "required": ["passed", "source_report", "snapshot_before", "snapshot_after", "evidence", "issues"],
    "properties": #{
        "passed": #{ "type": "boolean" }, "source_report": #{ "type": "object" },
        "snapshot_before": snapshot_schema, "snapshot_after": snapshot_schema,
        "evidence": #{ "type": "string" }, "issues": strings,
    },
};
let repair_schema = #{
    "type": "object", "required": ["reports"],
    "properties": #{ "reports": #{ "type": "array", "items": implementation_schema } },
};
let review_schema = #{
    "type": "object", "required": ["passed", "scope_ok", "api_unchanged", "behavior_unchanged", "reviewed_crates", "evidence", "findings"],
    "properties": #{
        "passed": #{ "type": "boolean" }, "scope_ok": #{ "type": "boolean" },
        "api_unchanged": #{ "type": "boolean" }, "behavior_unchanged": #{ "type": "boolean" },
        "reviewed_crates": strings, "evidence": #{ "type": "string" }, "findings": strings,
    },
};
let command_schema = #{
    "type": "object", "required": ["kind", "crate_name", "command", "exit_code", "evidence", "target_dir"],
    "properties": #{
        "kind": #{ "type": "string", "enum": ["fmt", "diff", "clippy", "unit-test", "bpm-boundary"] },
        "crate_name": #{ "type": "string" }, "command": #{ "type": "string" },
        "exit_code": #{ "type": "integer" }, "evidence": #{ "type": "string" },
        "target_dir": #{ "type": "string" },
    },
};
let verification_schema = #{
    "type": "object",
    "required": ["passed", "head", "scope_ok", "api_unchanged", "behavior_unchanged", "changed_files", "commands", "fingerprint_before", "fingerprint_after", "issues", "report", "compile_target_dir", "external_volume_mounted"],
    "properties": #{
        "passed": #{ "type": "boolean" }, "head": #{ "type": "string" },
        "scope_ok": #{ "type": "boolean" }, "api_unchanged": #{ "type": "boolean" },
        "behavior_unchanged": #{ "type": "boolean" }, "changed_files": strings,
        "commands": #{ "type": "array", "items": command_schema },
        "fingerprint_before": #{ "type": "string" }, "fingerprint_after": #{ "type": "string" },
        "issues": strings, "report": #{ "type": "string" },
        "compile_target_dir": #{ "type": "string" }, "external_volume_mounted": #{ "type": "boolean" },
    },
};

fn text(value) { type_of(value) == "string" && value != "" }
fn component(value) {
    if !text(value) { return false; }
    for c in value {
        if !"abcdefghijklmnopqrstuvwxyz0123456789-_".contains(c) { return false; }
    }
    true
}
fn crate_path(item) {
    if item == () || !component(item.name) || !text(item.path) { return false; }
    let parts = item.path.split("/");
    parts.len() == 4 && parts[0] == "backend" && parts[1] == "crates"
        && component(parts[2]) && parts[3] == item.name
}
fn source_file(path, scopes) {
    if !text(path) || !path.ends_with(".rs") || path.contains("\\") { return false; }
    for part in path.split("/") {
        if part == "" || part == "." || part == ".." { return false; }
    }
    for scope in scopes {
        if path.starts_with(scope.path + "/src/") { return true; }
    }
    false
}
fn repository_root_ok(root) {
    if !text(root) || !root.starts_with("/") || root.ends_with("/") || root.contains("\\") { return false; }
    let parts = root.split("/");
    let index = 0;
    for part in parts {
        if index > 0 && (part == "" || part == "." || part == "..") { return false; }
        index += 1;
    }
    true
}
fn relative_source_path(path, root) {
    if type_of(path) == "string" && repository_root_ok(root) && path.starts_with(root + "/") {
        return path.sub_string(root.len() + 1);
    }
    path
}
fn normalize_report(report, root) {
    if type_of(report) != "map" { return report; }
    for key in ["inspected_files", "files_changed"] {
        if type_of(report[key]) == "array" {
            let files = [];
            for file in report[key] { files.push(relative_source_path(file, root)); }
            report[key] = files;
        }
    }
    if type_of(report.changes) == "array" {
        let changes = [];
        for item in report.changes {
            if type_of(item) == "map" && type_of(item.files) == "array" {
                let files = [];
                for file in item.files { files.push(relative_source_path(file, root)); }
                item.files = files;
            }
            changes.push(item);
        }
        report.changes = changes;
    }
    report
}
fn usable(result) { result != () && result.success == true && type_of(result.output) == "map" }
fn normalize_verification(result, root) {
    if usable(result) && type_of(result.output.changed_files) == "array" {
        let files = [];
        for file in result.output.changed_files { files.push(relative_source_path(file, root)); }
        result.output.changed_files = files;
    }
    result
}
fn snapshot_ok(snapshot, head, outside_status) {
    type_of(snapshot) == "map" && snapshot.head == head && type_of(snapshot.status) == "string"
        && snapshot.outside_status == outside_status && snapshot.index_clean == true
        && type_of(snapshot.backend_untracked) == "array" && snapshot.backend_untracked.len() == 0
        && text(snapshot.fingerprint)
}
fn recovery_source_ok(source, scopes, head) {
    if type_of(source) != "map" || type_of(source.inventory) != "map"
        || source.inventory.backend_clean != true || source.inventory.head != head
        || type_of(source.completed) != "array" || source.completed.len() != 0
        || type_of(source.batch) != "array" || source.batch.len() == 0 || source.batch.len() > scopes.len()
        || type_of(source.results) != "array" || source.results.len() != source.batch.len() { return false; }
    let index = 0;
    for scope in source.batch {
        if scope != scopes[index] || !usable(source.results[index]) { return false; }
        index += 1;
    }
    true
}
fn implementation_ok(report, scope) {
    if report == () || report.crate_name != scope.name || !text(report.evidence)
        || type_of(report.inspected_files) != "array" || report.inspected_files.len() == 0
        || type_of(report.files_changed) != "array" || type_of(report.changes) != "array"
        || type_of(report.deferred) != "array" { return false; }
    if report.status != "changed" && report.status != "unchanged" { return false; }
    if report.status == "changed" && (report.files_changed.len() == 0 || report.changes.len() == 0) { return false; }
    if report.status == "unchanged" && (report.files_changed.len() != 0 || report.changes.len() != 0) { return false; }
    for file in report.inspected_files { if !source_file(file, [scope]) { return false; } }
    let changed_paths = [];
    for file in report.files_changed {
        if !source_file(file, [scope]) || changed_paths.contains(file) { return false; }
        changed_paths.push(file);
    }
    for change in report.changes {
        if !["过度防御", "过度抽象", "死代码", "无意义包装", "重复逻辑"].contains(change.category)
            || !text(change.summary) || !text(change.equivalence)
            || type_of(change.files) != "array" || change.files.len() == 0 { return false; }
        for file in change.files {
            if !source_file(file, [scope]) || !report.files_changed.contains(file) { return false; }
        }
    }
    for file in report.files_changed {
        let explained = false;
        for change in report.changes { if change.files.contains(file) { explained = true; } }
        if !explained { return false; }
    }
    if report.compiled != true && report.compiled != false { return false; }
    if type_of(report.compile_target_dir) != "string" { return false; }
    if report.compiled == true {
        if report.compile_target_dir != external_target_dir() { return false; }
    } else if report.compile_target_dir != "" {
        return false;
    }
    true
}
fn review_ok(result, scopes) {
    if !usable(result) { return false; }
    let output = result.output;
    if output.passed != true || output.scope_ok != true || output.api_unchanged != true
        || output.behavior_unchanged != true || !text(output.evidence)
        || type_of(output.findings) != "array" || output.findings.len() != 0
        || type_of(output.reviewed_crates) != "array" || output.reviewed_crates.len() != scopes.len() { return false; }
    for scope in scopes { if !output.reviewed_crates.contains(scope.name) { return false; } }
    true
}
fn external_target_dir() { "/Volumes/Kingston/rust-build-target" }
fn compile_command(body) { "CARGO_TARGET_DIR=" + external_target_dir() + " " + body }
fn expected_command(kind, crate_name) {
    if kind == "diff" { return "git diff --check"; }
    if kind == "fmt" { return "cargo fmt --all -- --check"; }
    if kind == "clippy" { return compile_command("cargo clippy -p " + crate_name + " --lib --bins --locked -- -D warnings"); }
    if kind == "unit-test" { return compile_command("env -u ERP_TEST_MONGO_URI cargo test -p " + crate_name + " --lib --locked"); }
    if kind == "bpm-boundary" { return "./scripts/check-bpm-boundaries.sh"; }
    ""
}
fn command_ok(commands, kind, crate_name) {
    for cmd in commands {
        if cmd.kind == kind && cmd.crate_name == crate_name && cmd.exit_code == 0
            && cmd.command == expected_command(kind, crate_name) && text(cmd.evidence) {
            if kind == "clippy" || kind == "unit-test" {
                if cmd.target_dir == external_target_dir() { return true; }
            } else if cmd.target_dir == "" {
                return true;
            }
        }
    }
    false
}
fn verification_ok(result, scopes, head, reports) {
    if !usable(result) { return false; }
    let output = result.output;
    if output.passed != true || output.head != head || output.scope_ok != true
        || output.api_unchanged != true || output.behavior_unchanged != true
        || !text(output.fingerprint_before) || output.fingerprint_before != output.fingerprint_after
        || type_of(output.changed_files) != "array" || type_of(output.commands) != "array"
        || type_of(output.issues) != "array" || output.issues.len() != 0 || !text(output.report) { return false; }
    if !command_ok(output.commands, "diff", "") { return false; }
    let declared = [];
    for report in reports {
        for file in report.files_changed { if !declared.contains(file) { declared.push(file); } }
    }
    if declared.len() != output.changed_files.len() { return false; }
    let observed = [];
    for file in output.changed_files {
        if !source_file(file, scopes) || !declared.contains(file) || observed.contains(file) { return false; }
        observed.push(file);
    }
    for file in declared { if !observed.contains(file) { return false; } }
    if type_of(output.compile_target_dir) != "string" { return false; }
    if output.changed_files.len() == 0 {
        if output.compile_target_dir != "" { return false; }
        return true;
    }
    if output.external_volume_mounted != true || output.compile_target_dir != external_target_dir() { return false; }
    if !command_ok(output.commands, "fmt", "") { return false; }
    for scope in scopes {
        let changed = false;
        for file in output.changed_files { if source_file(file, [scope]) { changed = true; } }
        if changed {
            if !command_ok(output.commands, "clippy", scope.name)
                || !command_ok(output.commands, "unit-test", scope.name) { return false; }
            if (scope.name == "bpm" || scope.name == "erp-workflow")
                && !command_ok(output.commands, "bpm-boundary", "") { return false; }
        }
    }
    true
}
fn tracked_diff_ok(diff, allowed_files, scopes) {
    if type_of(diff) != "string" { return false; }
    let headers = 0;
    let targets = 0;
    let in_hunk = false;
    for line in diff.split("\n") {
        if line.starts_with("diff --git ") { in_hunk = false; }
        if line.starts_with("@@ ") { in_hunk = true; }
        if in_hunk { continue; }
        if line.starts_with("diff --git ") {
            let valid = false;
            for file in allowed_files {
                if source_file(file, scopes) && line == "diff --git a/" + file + " b/" + file { valid = true; }
            }
            if !valid { return false; }
            headers += 1;
        }
        if line.starts_with("+++ ") {
            let valid = false;
            for file in allowed_files { if line == "+++ b/" + file { valid = true; } }
            if !valid { return false; }
            targets += 1;
        }
        if line.starts_with("deleted file mode ") || line.starts_with("new file mode ")
            || line.starts_with("old mode ") || line.starts_with("new mode ")
            || line.starts_with("rename from ") || line.starts_with("rename to ")
            || line.starts_with("copy from ") || line.starts_with("copy to ")
            || line.starts_with("Binary files ") || line.starts_with("GIT binary patch") { return false; }
    }
    diff == "" || (headers > 0 && headers == targets)
}
fn partition_diff(diff) {
    let backend_diff = "";
    let outside_diff = "";
    let backend_block = false;
    let lines = diff.split("\n");
    let index = 0;
    for line in lines {
        if line.starts_with("diff --git ") {
            backend_block = line.starts_with("diff --git a/backend/")
                || line.starts_with("diff --git \"a/backend/") || line.contains(" b/backend/")
                || line.contains(" \"b/backend/");
        }
        let ending = if index + 1 < lines.len() { "\n" } else { "" };
        if backend_block { backend_diff += line + ending; } else { outside_diff += line + ending; }
        index += 1;
    }
    #{ backend: backend_diff, outside: outside_diff }
}
fn tracked_scope_ok(diff, initial_outside_diff, allowed_files, scopes) {
    let parts = partition_diff(diff);
    parts.outside == initial_outside_diff && tracked_diff_ok(parts.backend, allowed_files, scopes)
}
fn reported_files(reports) {
    let files = [];
    for report in reports {
        for file in report.files_changed { if !files.contains(file) { files.push(file); } }
    }
    files
}
fn finish(status, summary, body, details) {
    let data_path = write_scratch_file("details.json", json_encode(details));
    let report = "# 后端 crate 精简报告\n\n状态：" + status + "\n\n" + summary + "\n\n" + body;
    if status == "blocked" { report += "\n\n运行已停止。工作区可能保留本次未验证的修改，请先查看差异并处理阻塞项。\n"; }
    let path = write_scratch_file("report.md", report);
    complete(#{ status: status, summary: summary, path: path, report: report, details_path: data_path });
}

let requested = [];
let parallelism = 4;
let recovery_file = "";
if args != () {
    if type_of(args) != "map" { finish("blocked", "参数必须是对象。", "使用 crates、parallelism 和 recovery_file 参数。", args); }
    for key in args.keys() {
        if key != "crates" && key != "parallelism" && key != "recovery_file" { finish("blocked", "存在未知参数：" + key, "支持 crates、parallelism 和 recovery_file。", args); }
    }
    if args.crates != () { requested = args.crates; }
    if args.parallelism != () { parallelism = args.parallelism; }
    if args.recovery_file != () { recovery_file = args.recovery_file; }
}
if type_of(requested) != "array" || type_of(parallelism) != "i64" || type_of(recovery_file) != "string" {
    finish("blocked", "参数类型错误。", "crates 使用包名数组；parallelism 使用 1..4 的整数；recovery_file 使用已授权的首批 details.json 路径。", args);
}
if parallelism < 1 || parallelism > 4 { finish("blocked", "parallelism 超出 1..4。", "默认并发为 4。", args); }
let requested_names = [];
for name in requested {
    if !component(name) { finish("blocked", "crate 包名无效。", "使用 backend workspace 中的包名。", requested); }
    if !requested_names.contains(name) { requested_names.push(name); }
}

let policy = "你在 ERP 仓库当前工作区执行保行为的 crate 内部精简。用工具读取实际文件，再做判断。先读 backend/AGENTS.md、backend/Cargo.toml、backend/crates/README.md、目标 crate 的 Cargo.toml/README.md，以及适用的目录规则与 rust-coding-standards skill。本地现行规则优先于旧记忆；README 与 AGENTS 的门禁有冲突时遵循 backend/AGENTS.md。\n";
policy += "写入范围仅限分配的 crate 下 src/**/*.rs；允许在同一 crate 调整私有模块及内联 mod tests。保持公开 Rust API 完全兼容：名称、可见性、签名、trait/泛型/生命周期约束、类型别名、re-export、宏生成接口与 Send/Sync 等能力均保持。公开 1:1 别名和零构造点的公开错误变体继续保留。读取全仓调用方用于取证，修改保持在本 crate。\n";
policy += "所有报告的 inspected_files、files_changed、changes[].files 和 changed_files 一律使用仓库相对路径 backend/crates/<分组>/<包名>/src/...rs，禁止绝对路径、./、反斜杠和 .. 路径段。工具可用绝对路径读取，报告字段按 git rev-parse --show-toplevel 转为仓库相对路径。\n";
policy += "保留全部可观察语义：正常/失败/边界返回值、错误变体/字段/文案/优先级、授权和数据范围、序列化和持久化字段、Mongo 查询/索引/排序/分页/聚合、金额精度、状态迁移、版本检查、幂等/并发/CAS、审计、事务边界/Executor、receipt-first 与其他必要读写顺序、外部副作用及其次数。保留 fail-closed 安全机制、输入/存量数据边界验证、HMAC/加密、重试上限、限流和恢复路径。可恢复 Err 继续返回；保持现有 panic 边界。\n";
policy += "五个方向逐项检查：①过度防御仅删除有完整调用链/类型不变式证明的重复检查，证明覆盖外部输入、历史数据、极值、并发与 feature/cfg 路径。②过度抽象优先移除私有单实现且无语义职责的层次，保留必要 Port、领域边界与测试接缝。③死代码用全仓引用、宏展开/注册/条件编译和入口可达性取证；零文本引用本身不足以删除公开或动态使用项。④无意义包装仅内联不承载错误映射、授权、事务、审计、生命周期或协议语义的私有透传。⑤在同 crate 对至少两处真实同语义重复提取最小 helper，保持不同业务规则独立，避免为省行数新增泛型框架、宏或 bool 开关。\n";
policy += "以净降低实现复杂度为目标，保留有用文档与行为测试。证据不足的候选记入 deferred 并保持原样；报告允许 changed/unchanged，逐项说明依据。新增/调整单测执行真实生产逻辑，覆盖成功、失败及相关边界；使用内存替身与既有窄 Port。源码字符串/方法名扫描不能充当行为测试。\n";
policy += "Cargo.toml/Cargo.lock、依赖与 feature、build.rs、非 Rust 数据文件、apps、config、脚本、文档、前端、其他 crate 和历史归档保持原样。保持文件存在，不删除整文件；涉及整文件删除的候选列为待确认。禁止提交/推送、分支/工作树操作、stash/reset/restore/checkout/clean、修改或覆盖既有用户工作、放宽 lint/门禁或添加 allow 掩盖问题。运行任何 python3 命令（包括清点、边界或脚本检查）时必须带 python3 -B 或 PYTHONDONTWRITEBYTECODE=1，禁止生成或改写仓库内 __pycache__/*.pyc 字节码文件。临时自用文件放 /tmp 并清理。\n";
policy += "开发迭代只按需跑当前 crate 的定向 --lib 单测，同一轮编译使用一种检查。收尾仅 rustfmt 实际修改的文件，使用 manifest 的 edition 与 skip_children=true，避免格式化递归改动未触及文件。方法/文件体积遵循现行 AGENTS，对未触及的历史问题只报告。集成测试、tests/、--test、--include-ignored、examples 和真实 Mongo/S3/外部服务均不执行；也不启动应用、浏览器或生成器。最终门禁由验证阶段统一串行执行。\n";
policy += "crate 需要独立编译时（cargo test、cargo clippy、cargo check，含 --lib），编译目录只能是已挂载外置盘 " + external_target_dir() + "。执行前用 mount 或 df 确认 /Volumes/Kingston 是已挂载卷；未挂载就不要 mkdir 该路径，也不要编译，报告置为阻塞。编译命令必须显式带 CARGO_TARGET_DIR=" + external_target_dir() + "，禁止 --target-dir，也禁止把 CARGO_TARGET_DIR 指到仓库、家目录、/tmp、backend/target 或任何内置盘路径。锁冲突只在这一外置目录重试，不为避开锁另建编译目录。未执行编译时 compiled 为 false 且 compile_target_dir 为空；实际编译过则为 true，且 compile_target_dir 等于该外置路径。cargo fmt、git diff 和边界脚本不产生编译目录。\n";

phase("清点");
let inventory_prompt = policy;
inventory_prompt += "本阶段只读。用终端运行 git rev-parse HEAD、git status --porcelain=v1 --untracked-files=all，并用 Python pathlib/tomllib 读取 backend/Cargo.toml 的 workspace.members，按 manifest 中 [package].name 列出规范化路径位于 backend/crates/ 下的全部活动成员。展开成员 glob 并处理 workspace.exclude；核对 backend/crates 下 Cargo.toml 与活动成员，发现遗漏/重复/软链接逃逸即报告阻塞。路径使用仓库相对形式 backend/crates/<分组>/<包名>，按 path 排序。避免从历史提交或归档发现目标；发现当前布局不支持时停止，保留证据。\n";
inventory_prompt += "repository_root 必须来自 git rev-parse --show-toplevel 的实际绝对路径，不带结尾斜杠。inventory_ok 只在成员完整性、路径与目录规则检查全部通过时为 true。返回 head、repository_root、inventory_ok、backend_clean（backend 下含未跟踪文件在内完全无既有修改且清点无阻塞才为 true）、outside_status（backend 之外的初始 porcelain 行，原样保留）、crates 和 evidence（实际命令、清点完整性与路径校验依据）。任一前提不满足时 backend_clean=false 并解释，保持所有文件原样。";
if recovery_file != "" { inventory_prompt += "\n本次显式请求恢复：backend 可能存在上一批修改。仍按事实填写 backend_clean；完整清点并保留现场，由后续恢复校验决定是否可接受。"; }
let inventory = agent(inventory_prompt, #{ label: "inventory", capability_mode: "execute", output_schema: inventory_schema });
if !usable(inventory) || type_of(inventory.output.crates) != "array"
    || inventory.output.inventory_ok != true
    || (recovery_file == "" && inventory.output.backend_clean != true) || !text(inventory.output.head)
    || !repository_root_ok(inventory.output.repository_root)
    || type_of(inventory.output.outside_status) != "string" || !text(inventory.output.evidence) {
    finish("blocked", "清点未确认完整清单与干净的后端基线。", "请处理清点结果后重新运行；已有修改由用户自行保留或处理。", inventory);
}
let all_crates = [];
let names = [];
let paths = [];
for item in inventory.output.crates {
    if !crate_path(item) || names.contains(item.name) || paths.contains(item.path) {
        finish("blocked", "crate 清单存在范围外、布局不支持或重复项。", "未启动精简阶段。", inventory.output);
    }
    names.push(item.name);
    paths.push(item.path);
    all_crates.push(item);
}
let scopes = [];
for name in requested_names {
    if !names.contains(name) { finish("blocked", "清单中找不到包：" + name, "未启动精简阶段。", inventory.output); }
}
for item in all_crates {
    if requested_names.len() == 0 || requested_names.contains(item.name) { scopes.push(item); }
}
if scopes.len() == 0 { finish("blocked", "没有可处理的 crate。", "检查 backend workspace 与目标范围。", inventory.output); }
let batches = (scopes.len() + parallelism - 1) / parallelism;
let maximum_calls = 1 + scopes.len() + 3 * batches + 3;
if recovery_file != "" { maximum_calls += 8; }
log("目标 " + scopes.len().to_string() + " 个 crate；最大并发 " + parallelism.to_string() + "；含修正重试最多 " + maximum_calls.to_string() + " 次 agent 调用。");
if budget().total < maximum_calls {
    finish("blocked", "执行预算不足，尚未修改代码。", "请将 agent_budget 提升到至少 " + maximum_calls.to_string() + "，或用 crates 缩小范围。", #{ inventory: inventory.output, maximum_calls: maximum_calls });
}
let baseline = inventory.output.head;
let initial_diff = git_diff_since(baseline);
let initial_parts = partition_diff(initial_diff);
if recovery_file == "" && initial_parts.backend != "" {
    finish("blocked", "宿主检测到后端已有跟踪文件差异。", "停止精简并保留现有工作。", #{ inventory: inventory.output, diff: initial_diff });
}
let outside_diff = initial_parts.outside;
let recovered_reports = [];
let recovery_snapshot = ();
let snapshot_prompt = policy;
snapshot_prompt += "本阶段仅只读采集工作区快照，不修改文件、不格式化、不编译。必须逐字执行下面的同一 Python 代码（通过 python3 内联输入），返回其 JSON 输出，禁止自行换哈希算法、前缀、顺序或删改原始 stdout 字节。所有恢复和复核快照共用此实现。执行失败直接报告失败，不伪造快照。\n";
let snapshot_code = "import hashlib, json, os, pathlib, subprocess\nroot = pathlib.Path(subprocess.check_output([\"git\", \"rev-parse\", \"--show-toplevel\"]).decode().strip()).resolve()\ndef git(*args):\n    return subprocess.check_output([\"git\", *args], cwd=root)\nhead = git(\"rev-parse\", \"HEAD\")\nstatus = git(\"status\", \"--porcelain=v1\", \"--untracked-files=all\", \"-z\")\nhash_value = hashlib.sha256()\ndef add(data):\n    hash_value.update(len(data).to_bytes(8, \"big\"))\n    hash_value.update(data)\nfor data in [head, status, git(\"diff\", \"--binary\", \"HEAD\"), git(\"diff\", \"--cached\", \"--binary\")]:\n    add(data)\nfor raw_path in sorted(p for p in git(\"ls-files\", \"--others\", \"--exclude-standard\", \"-z\").split(b\"\\0\") if p):\n    path = root / os.fsdecode(raw_path)\n    if path.is_symlink() or not path.resolve().is_relative_to(root):\n        raise RuntimeError(\"unsafe untracked path\")\n    add(raw_path)\n    add(path.read_bytes())\nstatus_text = git(\"status\", \"--porcelain=v1\", \"--untracked-files=all\").decode()\noutside = \"\".join(line for line in status_text.splitlines(keepends=True) if not line[3:].startswith(\"backend/\"))\nuntracked = [os.fsdecode(p) for p in git(\"ls-files\", \"--others\", \"--exclude-standard\", \"-z\", \"--\", \"backend\").split(b\"\\0\") if p]\nif head != git(\"rev-parse\", \"HEAD\") or status != git(\"status\", \"--porcelain=v1\", \"--untracked-files=all\", \"-z\"):\n    raise RuntimeError(\"workspace changed during snapshot\")\nprint(json.dumps({\"head\": head.decode().strip(), \"status\": status_text, \"outside_status\": outside, \"index_clean\": not git(\"diff\", \"--cached\", \"--name-only\"), \"backend_untracked\": untracked, \"fingerprint\": hash_value.hexdigest()}, ensure_ascii=False))\n";
snapshot_prompt += snapshot_code;
if recovery_file != "" {
    let recovery_prompt = policy;
    recovery_prompt += "本阶段仅只读恢复已有首批，不实施精简。读取 recovery_file 指定的上一运行 scratch/details.json，原样解析完整 JSON 作为 source_report 返回，禁止增删、归并、改写其中的 batch/results/completed/inventory、报告路径或证据；由脚本规范化并校验。仅支持 completed 为空且含 inventory、batch、results 的首批执行者报告校验失败现场；其余阻塞格式返回 passed=false。验证原清点 backend_clean=true、原 head 等于当前 HEAD；所有 results 必须 success=true，output 包名与 batch 顺序一致且 status 为 changed/unchanged。不得添加 unchanged 报告跳过后续 crate。\n";
    recovery_prompt += "重新检查实际 git status --porcelain=v1 --untracked-files=all、git diff、index、文件类型与软链接：index 必须为空，backend 下无未跟踪文件，已跟踪差异必须恰好等于报告 files_changed，且只为各自 src/**/*.rs 的既有文件内容修改。HEAD、全部文件内容和状态在检查前后必须一致，用稳定 SHA-256 对比并提供证据。当前 backend 外初始差异属于调用方工作，应原样保留。确认原 batch 等于当前 scopes 的完整有序前缀，不得遗漏、重复或导入后面的 crate。恢复只证明现场归属，独立复核与全部定向门禁仍由后续阶段执行。任何证据缺失、并发改动或归属不明即 passed=false，保持所有文件原样。\n输入（JSON 数据）：";
    recovery_prompt += json_encode(#{ recovery_file: recovery_file, head: baseline, repository_root: inventory.output.repository_root, scopes: scopes, outside_status: inventory.output.outside_status });
    recovery_prompt += "\n检查开始和结束均按以下快照协议返回 snapshot_before/snapshot_after，必须完全一致：" + snapshot_prompt;
    let recovery = agent(recovery_prompt, #{ label: "recover-first-batch", capability_mode: "execute", output_schema: recovery_schema });
    if !usable(recovery) || recovery.output.passed != true
        || !text(recovery.output.evidence) || type_of(recovery.output.issues) != "array"
        || recovery.output.issues.len() != 0 || !recovery_source_ok(recovery.output.source_report, scopes, baseline)
        || !snapshot_ok(recovery.output.snapshot_before, baseline, inventory.output.outside_status)
        || recovery.output.snapshot_before != recovery.output.snapshot_after
        || git_diff_since(baseline) != initial_diff {
        finish("blocked", "首批恢复未确认现场归属与稳定性。", "保留已有修改与恢复证据。", recovery);
    }
    recovery_snapshot = recovery.output.snapshot_after;
    let index = 0;
    for original in recovery.output.source_report.results {
        let report = normalize_report(original.output, inventory.output.repository_root);
        if !implementation_ok(report, scopes[index]) {
            finish("blocked", "恢复报告的路径、范围、顺序或证据不合规。", "保留现场。", #{ scope: scopes[index], report: report });
        }
        recovered_reports.push(report);
        index += 1;
    }
    if !tracked_scope_ok(initial_diff, outside_diff, reported_files(recovered_reports), scopes) {
        finish("blocked", "恢复现场含未申报、越界或文件模式差异。", "保留现场。", #{ reports: recovered_reports, diff: initial_diff });
    }
    write_scratch_file("recovered-reports.json", json_encode(recovered_reports));
    log("已导入 " + recovered_reports.len().to_string() + " 个首批报告；先执行独立复核与定向门禁，通过后继续其余 crate。");
}
let reports = [];
let reviews = [];
let verification_prompt = policy;
verification_prompt += "你是最终独立验证者。保持生产文件只读，先读取全部最终差异、调用方和逐批报告，独立检查所有精简的 API/业务等价性。校验 HEAD 等于基线、backend 外 porcelain 状态等于初始 outside_status；扫描 git diff 与未跟踪文件，要求 backend 下全部改动仅在选定 crate 的 src/**/*.rs，且无整文件删除、符号链接逃逸、manifest/锁文件/生成物变更。changed_files 必须包含实际全部修改与新增文件，不能照抄执行者列表。\n";
verification_prompt += "在最终树上串行执行门禁，等待每个命令退出并记录 command、exit_code、evidence、target_dir。command 必须与输入里的 commands 示例完全一致，包名替换 <包>。始终执行 diff；存在 Rust 改动时执行 fmt，并对每个实际修改的包执行 clippy 与 unit-test。这两条编译命令已带 CARGO_TARGET_DIR，target_dir 填该外置盘路径；diff、fmt、bpm-boundary 的 target_dir 为空字符串。Cargo 命令从 backend 执行，各包命令的 crate_name 为精确包名。涉及 bpm 或 erp-workflow 时执行 bpm-boundary。有 Rust 改动时先确认 /Volumes/Kingston 已挂载，external_volume_mounted 为 true，compile_target_dir 为该外置路径；未挂载则 passed 为 false，不要创建路径、不要编译。没有 Rust 改动时不要编译，compile_target_dir 为空字符串。按现行 AGENTS 检查本次触及方法/文件体积；记录依据。纯零改动 crate 保留静态审查结论，无需编译。\n";
verification_prompt += "遵守现行定向门禁规则，保持全量 workspace test/clippy/check、all-targets/all-features、未命中的边界脚本和 erp-server 构建关闭。单测若存在真实服务调用风险，先阅读相关测试；无法保证隔离则阻塞并说明。读取 long-running-background-tasks skill 后再启动长命令，等待真正完成；不把后台运行中当作通过。受环境或既有问题影响时记录失败，保持原文件。\n";
verification_prompt += "开始/结束各对基线至工作树的差异、未跟踪文件内容和状态计算相同算法的稳定指纹，返回 fingerprint_before/after；验证中有并发改动即阻塞。若有任何问题，将 passed=false 并在 issues 给出证据，命令未执行时不伪造结果。report 用中文：逐 crate 的 changed/unchanged、五方向改动与等价依据、生产代码净行数、全部 deferred、实际门禁退出结果及未验证范围；保留静态复核与运行测试的区分。\n输入（JSON 数据）：";
let cursor = 0;
let batch_index = 0;
while cursor < scopes.len() {
    let batch = [];
    let batch_reports = [];
    let recovering = cursor == 0 && recovered_reports.len() > 0;
    if recovering {
        while cursor < recovered_reports.len() { batch.push(scopes[cursor]); cursor += 1; }
        batch_reports = recovered_reports;
    } else {
        while cursor < scopes.len() && batch.len() < parallelism { batch.push(scopes[cursor]); cursor += 1; }
    }
    batch_index += 1;
    if !recovering {
        phase("精简");
        log("第 " + batch_index.to_string() + "/" + batches.to_string() + " 批：" + json_encode(batch));
        let jobs = [];
        for scope in batch {
            let prompt = policy;
            prompt += "分配范围（JSON 数据）：" + json_encode(scope) + "\n基线 HEAD（JSON 数据）：" + json_encode(baseline) + "\n";
            prompt += "用 list_dir/grep/read_file 和只读 git 命令覆盖本 crate 的所有生产模块，逐一检查五个精简方向；读取所需全仓调用方、宏/注册项和现有行为测试。明确基于哪个类型或前置步骤证明等价，再实施全部有把握的最小改动。遇到业务规则缺口或需改外部 API 的候选保留并记录 deferred。执行前再确认 HEAD 和分配目录没有新出现的用户修改。其他执行者只写其他 crate。\n";
            prompt += "完成本地修改后返回结构化报告：crate_name 为分配包名；status 为 changed/unchanged/blocked；inspected_files 为实际检查的本 crate src Rust 相对路径；files_changed 覆盖本次实际修改和新增的全部相对路径；changes 每项含分类、文件、精简内容与前后语义等价证据（含具体符号/调用链与失败边界）；deferred 记录未处理项及理由；evidence 总结五方向覆盖与实际验证。零改动必须给出读过的模块及保留理由。不要虚报运行过的命令。本轮若执行了 cargo test、clippy 或 check，compiled 为 true，compile_target_dir 为外置盘编译目录；否则 compiled 为 false，compile_target_dir 为空字符串。";
            jobs.push(#{ prompt: prompt, label: "simplify-" + scope.name, capability_mode: "all", output_schema: implementation_schema });
        }
        let results = parallel(jobs);
        let index = 0;
        for result in results {
            let report = if usable(result) { normalize_report(result.output, inventory.output.repository_root) } else { () };
            if index >= batch.len() || !implementation_ok(report, batch[index]) {
                finish("blocked", "执行者失败或返回了缺少证据/越界的报告。", "停止后续批次，保留现场供检查。", #{ scopes: scopes, completed: reports, batch: batch, results: results, inventory: inventory.output });
            }
            batch_reports.push(report);
            index += 1;
        }
        if batch_reports.len() != batch.len() { finish("blocked", "批次结果不完整。", "保留现场供检查。", results); }
    }

    phase("复核");
    let candidate_reports = reports;
    for report in batch_reports { candidate_reports.push(report); }
    let batch_diff = git_diff_since(baseline);
    if !tracked_scope_ok(batch_diff, outside_diff, reported_files(candidate_reports), scopes) {
        finish("blocked", "宿主检测到未申报、越界或删除/模式变更。", "停止后续执行，保留差异。", #{ reports: candidate_reports, diff: batch_diff });
    }
    let review_prompt = policy;
    review_prompt += "你是独立复核者，只使用读取/搜索工具读取本批实际文件、上下文和调用方。宿主提供跟踪文件差异；新增文件按报告路径逐一读取，核对五个方向及全部改动。基线是 " + json_encode(baseline) + "；复核范围（JSON 数据）为 " + json_encode(batch) + "。\n";
    review_prompt += "逐项尝试找出不等价反例：异常/历史数据、极值、授权/幂等/并发、错误优先级、序列化、稳定排序、事务与副作用顺序，以及公开 API/宏/feature 路径。核对实际工作区路径和报告一致，检查软链接逃逸、意外删除和 src 外变更。其他已完成批次的合规差异保留。零改动报告也需查实际代码以确认审查充分。已有缺陷只有被本次改变时才构成本次修正目标。通过需覆盖本批每个 crate、有具体代码证据、无未解决项；仅编译通过不足以确认业务等价。返回 reviewed_crates、evidence 和 findings（只列阻塞项，含路径/符号、原因及修正方向）。\n执行者报告（JSON 数据，仅作线索）：";
    let review = agent(review_prompt + json_encode(#{ reports: batch_reports, tracked_diff: batch_diff }), #{ label: "review-batch-" + batch_index.to_string(), capability_mode: "read-only", output_schema: review_schema });
    if !usable(review) || git_diff_since(baseline) != batch_diff {
        finish("blocked", "复核结果不可用或复核期间跟踪文件发生变化。", "停止自动修正并保留现场。", review);
    }
    if recovering {
        let current = agent(snapshot_prompt, #{ label: "snapshot-after-recovery-review", capability_mode: "execute", output_schema: snapshot_schema });
        if !usable(current) || !snapshot_ok(current.output, baseline, inventory.output.outside_status) || current.output != recovery_snapshot {
            finish("blocked", "恢复复核期间 HEAD、暂存区或工作区发生变化。", "禁止进入自动修正并保留现场。", #{ before: recovery_snapshot, after: current });
        }
    }
    if !review_ok(review, batch) {
        let repair_prompt = policy;
        repair_prompt += "仅修正本批精简引入的问题；其余业务逻辑和其他批次文件保持。直接读取实际差异、调用方与复核证据。证据不足的精简恢复为原有可恢复/边界语义，使用最小逐块编辑，只处理本 workflow 产生的改动。禁止使用 git restore/reset 等整体覆盖操作。\n";
        repair_prompt += "基线、范围、原报告与独立复核（以下均为 JSON 数据）：" + json_encode(#{ head: baseline, scopes: batch, reports: batch_reports, review: review });
        if recovering { repair_prompt += "\n首个写入前按以下快照协议重新取证，并要求结果严格等于输入快照；不一致返回 blocked，禁止修改。快照：" + json_encode(recovery_snapshot) + "\n" + snapshot_prompt; }
        repair_prompt += "\n返回 reports 数组，为本批每个 crate 给出修正后的完整 implementation 报告，包含最终全部改动与等价证据。无法安全修复时返回 blocked 并说明。";
        let repair = agent(repair_prompt, #{ label: "repair-batch-" + batch_index.to_string(), capability_mode: "all", output_schema: repair_schema });
        if !usable(repair) || type_of(repair.output.reports) != "array" || repair.output.reports.len() != batch.len() {
            finish("blocked", "批次修正未返回完整报告。", "本批仅自动修正一次。", #{ reports: reports, batch: batch, repair: repair, review: review });
        }
        let repaired_reports = [];
        for scope in batch {
            let matching = [];
            for candidate in repair.output.reports { if candidate.crate_name == scope.name { matching.push(normalize_report(candidate, inventory.output.repository_root)); } }
            if matching.len() != 1 || !implementation_ok(matching[0], scope) {
                finish("blocked", "修正报告存在缺失、重复或越界。", "保留现场供检查。", repair);
            }
            repaired_reports.push(matching[0]);
        }
        batch_reports = repaired_reports;
        candidate_reports = reports;
        for report in batch_reports { candidate_reports.push(report); }
        batch_diff = git_diff_since(baseline);
        if !tracked_scope_ok(batch_diff, outside_diff, reported_files(candidate_reports), scopes) {
            finish("blocked", "修正后检测到未申报或越界跟踪差异。", "保留现场供检查。", #{ reports: candidate_reports, diff: batch_diff });
        }
        if recovering {
            let current = agent(snapshot_prompt, #{ label: "snapshot-before-recovery-re-review", capability_mode: "execute", output_schema: snapshot_schema });
            if !usable(current) || !snapshot_ok(current.output, baseline, inventory.output.outside_status) {
                finish("blocked", "恢复修正后的快照不合规。", "停止复核并保留现场。", current);
            }
            recovery_snapshot = current.output;
        }
        review = agent(review_prompt + json_encode(#{ reports: batch_reports, tracked_diff: batch_diff }), #{ label: "re-review-batch-" + batch_index.to_string(), capability_mode: "read-only", output_schema: review_schema });
        if !review_ok(review, batch) || git_diff_since(baseline) != batch_diff {
            finish("blocked", "批次独立复核仍未通过。", "停止后续批次，保留本次修改与复核证据。", #{ reports: reports, batch_reports: batch_reports, review: review });
        }
        if recovering {
            let current = agent(snapshot_prompt, #{ label: "snapshot-after-recovery-re-review", capability_mode: "execute", output_schema: snapshot_schema });
            if !usable(current) || current.output != recovery_snapshot {
                finish("blocked", "恢复再复核期间工作区发生变化。", "停止门禁并保留现场。", #{ before: recovery_snapshot, after: current });
            }
        }
    }
    for report in batch_reports { reports.push(report); }
    reviews.push(review.output);
    write_scratch_file("batch-" + batch_index.to_string() + ".json", json_encode(#{ crates: batch, reports: batch_reports, review: review.output }));
    if recovering {
        phase("验证");
        let recovery_diff = git_diff_since(baseline);
        let recovery_context = #{ baseline: baseline, outside_status: inventory.output.outside_status, scopes: batch, reports: batch_reports, reviews: [review.output] };
        recovery_context.commands = #{ diff: expected_command("diff", ""), fmt: expected_command("fmt", ""), clippy_example: expected_command("clippy", "<包>"), unit_test_example: expected_command("unit-test", "<包>"), bpm_boundary: expected_command("bpm-boundary", "") };
        let verified = normalize_verification(agent(verification_prompt + json_encode(recovery_context), #{ label: "verify-recovered-batch", capability_mode: "execute", output_schema: verification_schema }), inventory.output.repository_root);
        if !verification_ok(verified, batch, baseline, batch_reports) || git_diff_since(baseline) != recovery_diff {
            finish("blocked", "恢复首批的定向门禁未通过。", "后续 crate 尚未启动，保留现场与实际失败证据。", #{ context: recovery_context, verification: verified });
        }
        write_scratch_file("recovered-verification.json", json_encode(verified.output));
        log("恢复首批已通过独立复核和定向门禁；继续剩余 " + (scopes.len() - cursor).to_string() + " 个 crate。");
    }
}

phase("验证");
let validation_diff = git_diff_since(baseline);
if !tracked_scope_ok(validation_diff, outside_diff, reported_files(reports), scopes) {
    finish("blocked", "最终验证前检测到越界跟踪差异。", "停止并保留现场。", #{ reports: reports, diff: validation_diff });
}
let context = #{ baseline: baseline, outside_status: inventory.output.outside_status, scopes: scopes, reports: reports, reviews: reviews };
context.commands = #{ diff: expected_command("diff", ""), fmt: expected_command("fmt", ""), clippy_example: expected_command("clippy", "<包>"), unit_test_example: expected_command("unit-test", "<包>"), bpm_boundary: expected_command("bpm-boundary", "") };
let verification = normalize_verification(agent(verification_prompt + json_encode(context), #{ label: "verify-final-tree", capability_mode: "execute", output_schema: verification_schema }), inventory.output.repository_root);
let final_repair = ();
if !usable(verification) || git_diff_since(baseline) != validation_diff {
    finish("blocked", "最终验证结果不可用或验证期间跟踪文件发生变化。", "停止自动修正并保留现场。", verification);
}
if !verification_ok(verification, scopes, baseline, reports) {
    let repair_prompt = policy;
    repair_prompt += "最终验证遇到阻塞。仅修复本 workflow 引入且有代码/命令证据的回归，在当前已有改动的 crate 内做最小修正；读取基线与实际文件确认归属。预存问题、环境/工具失败、用户并发改动和无法确认语义等价的情况保持现场并返回 blocked，禁止放宽 lint/测试/权限或修改边界外文件。修正后按需补内联行为单测并只格式化触及文件；最终门禁交由新的独立验证者。\n输入（JSON 数据）：";
    repair_prompt += json_encode(#{ context: context, verification: verification });
    repair_prompt += "\n返回 reports 数组，仅列本次修正的 crate，按 implementation schema 说明修正后的全部差异与等价证据；无安全修正空间时返回空数组。";
    final_repair = agent(repair_prompt, #{ label: "repair-final-gates", capability_mode: "all", output_schema: repair_schema });
    if !usable(final_repair) || type_of(final_repair.output.reports) != "array" || final_repair.output.reports.len() == 0 {
        finish("blocked", "最终验证未通过，自动修正停止。", "具体失败与验证范围见运行记录。", #{ context: context, verification: verification, repair: final_repair });
    }
    let repaired_names = [];
    let normalized_repairs = [];
    for candidate in final_repair.output.reports {
        let report = normalize_report(candidate, inventory.output.repository_root);
        let found = false;
        for scope in scopes {
            if scope.name == report.crate_name && implementation_ok(report, scope) { found = true; }
        }
        if !found || repaired_names.contains(report.crate_name) { finish("blocked", "最终修正报告越界或无效。", "保留现场供检查。", final_repair); }
        repaired_names.push(report.crate_name);
        normalized_repairs.push(report);
    }
    final_repair.output.reports = normalized_repairs;
    let final_reports = [];
    for prior in reports {
        let latest = prior;
        for repaired in final_repair.output.reports {
            if repaired.crate_name == prior.crate_name { latest = repaired; }
        }
        final_reports.push(latest);
    }
    reports = final_reports;
    context.reports = reports;
    context.final_repair = final_repair.output;
    context.previous_verification = verification;
    validation_diff = git_diff_since(baseline);
    if !tracked_scope_ok(validation_diff, outside_diff, reported_files(reports), scopes) {
        finish("blocked", "最终修正后检测到越界跟踪差异。", "保留现场供检查。", #{ context: context, diff: validation_diff });
    }
    verification = normalize_verification(agent(verification_prompt + json_encode(context), #{ label: "re-verify-final-tree", capability_mode: "execute", output_schema: verification_schema }), inventory.output.repository_root);
}
if !verification_ok(verification, scopes, baseline, reports) {
    finish("blocked", "最终独立验证仍未通过。", "保留修改和全部检查证据，等待人工处理。", #{ context: context, verification: verification, repair: final_repair });
}
let final_diff = git_diff_since(baseline);
if final_diff != validation_diff || !tracked_scope_ok(final_diff, outside_diff, reported_files(reports), scopes) {
    finish("blocked", "最终跟踪差异与已验证版本不一致。", "保留现场供检查。", #{ context: context, diff: final_diff });
}
phase("报告");
finish("verified", "已完成 " + scopes.len().to_string() + " 个 crate 的五方向精简审查，最终改动通过独立复核与定向门禁。", verification.output.report, #{ context: context, verification: verification.output });
