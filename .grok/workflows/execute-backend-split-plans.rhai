let meta = #{
    name: "execute-backend-split-plans",
    description: "逐个执行 docs/split/backend 拆分计划：动态设计、lightweight 实现、独立 Review、测试通过后标记已解决",
    when_to_use: "需要按 docs/split/backend/**/plan.md 逐个完成 Rust 大文件拆分并验证无方案漂移时",
    phases: [
        #{ title: "Discover", detail: "选择未解决计划并检查工作区安全性" },
        #{ title: "Design", detail: "按 plan 声明风险确定性分配设计角色并定稿" },
        #{ title: "Implement", detail: "由 lightweight 代理在共享工作区实现" },
        #{ title: "Review", detail: "独立纯代码逻辑审查，严禁执行 Cargo；Fix 后全量或增量复核" },
        #{ title: "Resolve", detail: "Review 通过后在 plan.md 标记已解决" },
    ],
};

// 用法：
//   /workflow execute-backend-split-plans
//   /workflow execute-backend-split-plans args={plan:"docs/split/backend/services/src/projection/mod.rs/plan.md"}
//   /workflow execute-backend-split-plans args={max_plans:3, allow_dirty:true, max_fix_rounds:2}
//
// 参数：
//   plan             - 可选，明确指定单个 plan.md；省略时自动选择未解决计划
//   plan_root        - 计划根目录，默认 docs/split/backend
//   max_plans        - 每次最多处理计划数，默认 1，范围 1..5
//   max_fix_rounds   - Review 不通过后的 lightweight 修复轮数，默认 1，范围 0..3
//   allow_dirty      - 是否允许从非干净工作区开始，默认 false；false 时需要用户恢复确认
//
// 模型规则：设计、综合和 Review agent 均不指定 model，自动继承主会话模型。
// 轻量规则：未解决计划发现、显式 plan 元数据读取、代码实现、Review 后修复和状态标记均显式使用 agent_type="lightweight"。

let discovery_schema = #{
    "type": "object",
    "required": ["plans"],
    "properties": #{
        "plans": #{
            "type": "array",
            "maxItems": 100,
            "items": #{
                "type": "object",
                "required": ["path", "source_file", "risk", "reason"],
                "properties": #{
                    "path": #{ "type": "string" },
                    "source_file": #{ "type": "string" },
                    "risk": #{ "type": "string", "enum": ["low", "medium", "high"] },
                    "reason": #{ "type": "string" },
                },
            },
        },
    },
};

let preflight_schema = #{
    "type": "object",
    "required": ["dirty", "safe_to_continue", "dirty_files", "reason"],
    "properties": #{
        "dirty": #{ "type": "boolean" },
        "safe_to_continue": #{ "type": "boolean" },
        "dirty_files": #{ "type": "array", "maxItems": 100, "items": #{ "type": "string" } },
        "reason": #{ "type": "string" },
    },
};

let plan_metadata_schema = #{
    "type": "object",
    "required": ["executable", "reason", "source_file", "risk"],
    "properties": #{
        "executable": #{ "type": "boolean" },
        "reason": #{ "type": "string" },
        "source_file": #{ "type": "string" },
        "risk": #{ "type": "string", "enum": ["low", "medium", "high"] },
    },
};

let role_design_schema = #{
    "type": "object",
    "required": ["role", "summary", "deliverables", "cross_scope_blockers", "risks"],
    "properties": #{
        "role": #{
            "type": "string",
            "enum": [
                "module-boundaries", "contracts-and-verification",
                "api-behavior", "verification-side-effects"
            ],
        },
        "summary": #{ "type": "string" },
        "deliverables": #{
            "type": "array", "minItems": 1, "maxItems": 40,
            "items": #{
                "type": "object",
                "required": ["topic", "evidence", "recommendation"],
                "properties": #{
                    "topic": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                    "recommendation": #{ "type": "string" },
                },
            },
        },
        "cross_scope_blockers": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "risks": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
    },
};

let synthesis_schema = #{
    "type": "object",
    "required": [
        "implementation_plan", "expected_files", "public_api_invariants",
        "behavior_invariants", "verification_commands", "original_tests"
    ],
    "properties": #{
        "implementation_plan": #{ "type": "string" },
        "expected_files": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
        "public_api_invariants": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "behavior_invariants": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "verification_commands": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
        "original_tests": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
    },
};

let implementation_schema = #{
    "type": "object",
    "required": ["status", "summary", "changed_files", "commands", "test_results", "verification_passed"],
    "properties": #{
        "status": #{ "type": "string", "enum": ["implemented", "no_changes", "blocked"] },
        "summary": #{ "type": "string" },
        "changed_files": #{ "type": "array", "maxItems": 100, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "test_results": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "verification_passed": #{ "type": "boolean" },
        "blocker": #{ "type": "string" },
    },
};

let fix_schema = #{
    "type": "object",
    "required": [
        "status", "summary", "changed_files", "commands", "test_results", "verification_passed",
        "cleanup_only", "target_files_changed", "target_snapshot_fingerprint"
    ],
    "properties": #{
        "status": #{ "type": "string", "enum": ["implemented", "no_changes", "blocked"] },
        "summary": #{ "type": "string" },
        "changed_files": #{ "type": "array", "maxItems": 100, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "test_results": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "verification_passed": #{ "type": "boolean" },
        "cleanup_only": #{ "type": "boolean" },
        "target_files_changed": #{ "type": "boolean" },
        "target_snapshot_fingerprint": #{ "type": "string" },
        "blocker": #{ "type": "string" },
    },
};

let review_schema = #{
    "type": "object",
    "required": [
        "pass", "review_mode", "plan_match", "drift", "api_preserved",
        "behavior_preserved", "target_snapshot_fingerprint", "findings", "evidence"
    ],
    "properties": #{
        "pass": #{ "type": "boolean" },
        "review_mode": #{ "type": "string", "enum": ["full", "delta"] },
        "plan_match": #{ "type": "boolean" },
        "drift": #{ "type": "boolean" },
        "api_preserved": #{ "type": "boolean" },
        "behavior_preserved": #{ "type": "boolean" },
        "target_snapshot_fingerprint": #{ "type": "string" },
        "findings": #{
            "type": "array", "maxItems": 30,
            "items": #{
                "type": "object",
                "required": ["severity", "category", "file", "issue", "required_fix"],
                "properties": #{
                    "severity": #{ "type": "string", "enum": ["blocking", "non_blocking"] },
                    "category": #{
                        "type": "string",
                        "enum": ["implementation", "test", "scope_drift", "unknown"]
                    },
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                    "required_fix": #{ "type": "string" },
                },
            },
        },
        "evidence": #{ "type": "string" },
    },
};

let marker_schema = #{
    "type": "object",
    "required": ["marked", "plan", "summary"],
    "properties": #{
        "marked": #{ "type": "boolean" },
        "plan": #{ "type": "string" },
        "summary": #{ "type": "string" },
    },
};

fn valid_plan_path(p, root) {
    type_of(p) == "string"
        && p.starts_with(root + "/")
        && p.ends_with("/plan.md")
        && !p.contains("..")
}

fn add_result(results, plan_path, state, summary) {
    results.push(#{ plan: plan_path, state: state, summary: summary });
    results
}

fn uses_forbidden_cargo_target(command) {
    type_of(command) == "string" && (
        command.contains("CARGO_TARGET_DIR")
        || command.contains("--target-dir")
        || command.contains("target-dir=")
        || command.contains("target-dir =")
        || command.contains("/tmp/")
        || command.contains("$TMPDIR")
        || command.contains("${TMPDIR")
        || command.contains("mktemp")
    )
}

fn commands_use_forbidden_cargo_target(commands) {
    let found = false;
    let command_i = 0;
    while command_i < commands.len() {
        if uses_forbidden_cargo_target(commands[command_i]) { found = true; }
        command_i += 1;
    }
    found
}

let cargo_target_policy = "\n【硬性 Cargo target 规则】严禁创建、设置、使用或清理任何临时/自定义 Cargo target 目录，包括但不限于 `CARGO_TARGET_DIR`、`cargo --target-dir`、`.cargo/config*` 中的 `target-dir` 覆盖、`/tmp`、`$TMPDIR`、`mktemp`、scratch 或 worktree 专用 target。所有 Cargo 命令只能使用仓库默认 target 目录；即使为了隔离生成物、提速、绕过锁或磁盘问题也不得例外。若默认 target 目录不可用，必须报告 blocked/阻塞，不得改用临时 target。返回命令列表或测试证据时必须保留完整命令及其环境变量前缀，不得隐去。\n";

// ---------- 参数 ----------
let plan_root = "docs/split/backend";
let explicit_plan = ();
let max_plans = 1;
let max_fix_rounds = 1;
let allow_dirty = false;

if args != () {
    if args.plan_root != () { plan_root = args.plan_root; }
    if args.plan != () { explicit_plan = args.plan; }
    if args.max_plans != () { max_plans = args.max_plans; }
    if args.max_fix_rounds != () { max_fix_rounds = args.max_fix_rounds; }
    if args.allow_dirty != () { allow_dirty = args.allow_dirty; }
}
if max_plans < 1 { max_plans = 1; }
if max_plans > 5 { max_plans = 5; }
if max_fix_rounds < 0 { max_fix_rounds = 0; }
if max_fix_rounds > 3 { max_fix_rounds = 3; }
if !plan_root.starts_with("docs/split/backend") || plan_root.contains("..") {
    pause("verification", "args.plan_root 必须位于 docs/split/backend 下。");
}
if explicit_plan != () && !valid_plan_path(explicit_plan, plan_root) {
    pause("verification", "args.plan 必须是 plan_root 下的 */plan.md 相对路径。");
}

// =====================================================================
// Phase 1: Discover + preflight
// =====================================================================
phase("Discover");

let preflight_prompt = "检查当前 Git 工作区是否存在未提交修改。必须执行 `git status --short` 并读取变更路径；禁止修改文件。\n";
preflight_prompt += "safe_to_continue 仅在工作区干净，或所有变更显然都是已知生成物且不会被本 workflow 覆盖时为 true。\n";
preflight_prompt += "返回 dirty、safe_to_continue、dirty_files、reason。不要凭记忆。";
preflight_prompt += cargo_target_policy;
let preflight = agent(preflight_prompt, #{
    label: "preflight:git-status",
    capability_mode: "execute",
    output_schema: preflight_schema,
});
if preflight == () || !preflight.success {
    pause("infra", "无法完成 Git 工作区预检，请恢复工具后重跑。");
}
if preflight.output.dirty == true && preflight.output.safe_to_continue == false && !allow_dirty {
    await_user(
        "user",
        "检测到可能与拆分冲突的未提交修改：" + json_encode(preflight.output.dirty_files)
            + "。确认这些修改可保留并继续后，恢复此 workflow；或用 args.allow_dirty=true 明确授权。"
    );
}

let selected_plans = [];
if explicit_plan != () {
    selected_plans.push(#{ path: explicit_plan, source_file: "", risk: "", reason: "explicit args.plan" });
} else {
    let discover_prompt = "在仓库根目录枚举 `" + plan_root + "/**/plan.md`，选择尚未解决且具备可执行拆分方案的计划。\n";
    discover_prompt += "必须使用 find/grep/read_file 读取真实文件，不得凭记忆。跳过包含 `- 状态：**已解决**` 的计划；";
    discover_prompt += "跳过拆分结论不是 split、分析失败或仅列入清单但没有具体拆分建议的计划。\n";
    discover_prompt += "确认源文件真实存在，并从每个计划的“源文件”和“风险”字段原样提取 backend/... 相对路径及 low/medium/high；不得重新评估或改写风险。按以下优先级排序：low → medium → high；同风险下 M/S → L/XL；再按路径排序。\n";
    discover_prompt += "最多返回 100 个候选，字段为 path、source_file、risk、reason。所有 path 必须位于 `" + plan_root + "`。";
    discover_prompt += cargo_target_policy;
    let discovered = agent(discover_prompt, #{
        label: "discover:unresolved-plans",
        capability_mode: "execute",
        agent_type: "lightweight",
        output_schema: discovery_schema,
    });
    if discovered != () && discovered.success && discovered.output.plans != () {
        let dpi = 0;
        while dpi < discovered.output.plans.len() {
            let candidate = discovered.output.plans[dpi];
            if valid_plan_path(candidate.path, plan_root) {
                selected_plans.push(candidate);
            } else {
                log("丢弃越界或非法 plan 路径：" + candidate.path);
            }
            dpi += 1;
        }
    }
}

if selected_plans.len() == 0 {
    complete(#{
        summary: "没有找到可执行的未解决 backend 拆分计划。",
        processed: [],
        resolved: 0,
        blocked: 0,
    });
}
if selected_plans.len() > max_plans {
    let capped = [];
    let cap_i = 0;
    while cap_i < max_plans {
        capped.push(selected_plans[cap_i]);
        cap_i += 1;
    }
    log("候选计划 " + selected_plans.len().to_string() + " 个，本次按 max_plans 处理前 " + max_plans.to_string() + " 个。");
    selected_plans = capped;
}

let processed = [];
let resolved_count = 0;
let blocked_count = 0;
let plan_index = 0;

while plan_index < selected_plans.len() {
    let plan_item = selected_plans[plan_index];
    let plan_path = plan_item.path;
    log("开始处理计划 " + (plan_index + 1).to_string() + "/" + selected_plans.len().to_string() + "：" + plan_path);

    // =================================================================
    // Phase 2: Read declared plan metadata, then route deterministically
    // =================================================================
    phase("Design");

    let source_file = "";
    let risk_label = "";
    if type_of(plan_item.source_file) == "string" { source_file = plan_item.source_file; }
    if type_of(plan_item.risk) == "string" { risk_label = plan_item.risk; }

    // 自动发现已返回 plan 中声明的 source_file/risk；显式 plan 才补一次轻量字段读取。
    if source_file == "" || risk_label == "" {
        let metadata_prompt = "只读取并校验计划文件 `" + plan_path + "` 的声明元数据，不做设计、不重新评估风险、不决定代理数量。\n";
        metadata_prompt += "必须真实读取 plan，并确认：拆分结论为 split、存在具体拆分建议、尚未标记已解决、源文件字段为真实存在的 backend/... 相对路径。若代码已经完成拆分但 plan 尚未标记，仍可 executable=true。\n";
        metadata_prompt += "risk 必须原样返回 plan 文件声明的 low、medium 或 high；source_file 原样返回“源文件”字段。禁止修改文件。";
        metadata_prompt += cargo_target_policy;
        let metadata = agent(metadata_prompt, #{
            label: "plan-metadata:" + plan_index.to_string(),
            capability_mode: "read-only",
            agent_type: "lightweight",
            output_schema: plan_metadata_schema,
        });
        if metadata == () || !metadata.success || metadata.output.executable != true {
            let why = "无法读取计划元数据或计划不可执行";
            if metadata != () && metadata.success && metadata.output.reason != () {
                why = metadata.output.reason;
            }
            processed = add_result(processed, plan_path, "blocked", why);
            blocked_count += 1;
            plan_index += 1;
            continue;
        }
        source_file = metadata.output.source_file;
        risk_label = metadata.output.risk;
    }

    if !source_file.starts_with("backend/") || source_file.contains("..") {
        processed = add_result(processed, plan_path, "blocked", "plan 声明了非法 source_file");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }
    if risk_label != "low" && risk_label != "medium" && risk_label != "high" {
        processed = add_result(processed, plan_path, "blocked", "plan 风险必须是 low、medium 或 high");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    let designer_count = 1;
    if risk_label == "medium" { designer_count = 2; }
    if risk_label == "high" { designer_count = 3; }
    log("workflow 按 plan 声明风险 " + risk_label + " 确定使用 " + designer_count.to_string() + " 个同模型设计代理。");

    let design_roles = [];
    if designer_count == 1 {
        design_roles.push("integrated");
    } else if designer_count == 2 {
        design_roles.push("module-boundaries");
        design_roles.push("contracts-and-verification");
    } else {
        design_roles.push("module-boundaries");
        design_roles.push("api-behavior");
        design_roles.push("verification-side-effects");
    }

    let design_jobs = [];
    let designer_i = 0;
    while designer_i < design_roles.len() {
        let role = design_roles[designer_i];
        let role_scope = "";
        if role == "integrated" {
            role_scope = "这是单设计代理路径。完整覆盖文件/符号边界、API 与行为不变量、调用方和验证命令，但保持结论精炼，避免重复转述 plan。";
        } else if role == "module-boundaries" {
            role_scope = "只分析模块边界：逐文件符号归属、imports/trait scope、可见性与 re-export、私有 helper 归属、依赖方向和循环依赖。不要完整重做事务、业务行为或测试矩阵；跨范围问题只写入 cross_scope_blockers。";
        } else if role == "contracts-and-verification" {
            role_scope = "只分析 API/行为契约与验证：调用方兼容、DTO/返回类型、查询与投影、事务/幂等/审计/外部调用顺序、错误语义、原始测试、质量门禁和命令副作用。不要重复完整文件符号迁移表；结构阻塞只写入 cross_scope_blockers。";
        } else if role == "api-behavior" {
            role_scope = "只分析公开 API、调用方、可见性契约、DTO/返回类型、数据库查询与投影、事务/幂等/审计/外部调用顺序、错误与安全语义。不要输出完整文件迁移表或测试矩阵；跨范围问题只写入 cross_scope_blockers。";
        } else {
            role_scope = "只分析验证与工作区副作用：原始测试、契约测试、格式/编译/Clippy/测试门禁、生成物和 build script、副作用命令、验证前后 git diff 基线。不要重做文件拆分或完整业务分析；跨范围问题只写入 cross_scope_blockers。";
        }

        let design_prompt = "";
        let design_output_schema = role_design_schema;
        if role == "integrated" {
            design_output_schema = synthesis_schema;
            design_prompt = "你是代码拆分的一体化设计代理，使用与主会话相同的模型。只读，不得修改文件。你是本路径唯一的设计代理，将直接产出最终实施方案，不会再有 synthesize 代理。\n";
            design_prompt += "计划：`" + plan_path + "`；源文件：`" + source_file + "`。\n";
            design_prompt += role_scope + "\n";
            design_prompt += "必须真实读取 plan、相关源码、调用方、测试、AGENTS.md 与适用编码规范；implementation_plan 中的关键判断必须附真实路径或符号依据。\n";
            design_prompt += "直接输出可供冷启动 lightweight 实现代理执行的最终方案：implementation_plan 给出有顺序的文件与符号迁移步骤；expected_files 列出精确变更范围；public_api_invariants 与 behavior_invariants 分别列出必须保持的契约；verification_commands 合并安全的质量门禁；original_tests 列出拆分前已有相关测试。\n";
            design_prompt += "方案应完整但精炼，不得假设后续代理会补齐遗漏。所有建议都必须保持纯拆分：不得顺手修改业务规则、错误语义、公开 API、DTO、数据库查询、事务顺序或生成逻辑。";
        } else {
            design_prompt = "你是互补职责的代码拆分设计代理，使用与主会话相同的模型。只读，不得修改文件。\n";
            design_prompt += "计划：`" + plan_path + "`；源文件：`" + source_file + "`；你的唯一角色：" + role + "。\n";
            design_prompt += role_scope + "\n";
            design_prompt += "必须真实读取 plan、角色所需源码/调用方/测试、AGENTS.md 与适用编码规范，并在 evidence 中给出真实路径或符号依据。\n";
            design_prompt += "deliverables 只能覆盖本角色范围，不得为了显得完整而复制其他角色的分析；发现跨范围阻塞时只记录到 cross_scope_blockers，交给综合代理处理。\n";
            design_prompt += "所有建议都必须保持纯拆分：不得顺手修改业务规则、错误语义、公开 API、DTO、数据库查询、事务顺序或生成逻辑。role 字段必须精确返回 `" + role + "`。";
        }
        design_prompt += cargo_target_policy;
        design_jobs.push(#{
            prompt: design_prompt,
            label: "design:" + plan_index.to_string() + ":" + role,
            capability_mode: "read-only",
            output_schema: design_output_schema,
        });
        designer_i += 1;
    }

    let design_results = parallel(design_jobs);
    let usable_designs = [];
    let dr_i = 0;
    while dr_i < design_results.len() {
        let dr = design_results[dr_i];
        let usable = false;
        if designer_count == 1 {
            usable = dr != () && dr.success && dr.output.implementation_plan != ();
        } else {
            usable = dr != () && dr.success && dr.output.summary != ()
                && dr.output.role == design_roles[dr_i];
        }
        if usable {
            usable_designs.push(dr.output);
        } else {
            log("丢弃失败或格式不匹配的设计结果：" + design_roles[dr_i]);
        }
        dr_i += 1;
    }
    if usable_designs.len() == 0 {
        processed = add_result(processed, plan_path, "blocked", "所有设计代理均未返回可用方案");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    let final_plan = ();
    if designer_count == 1 {
        final_plan = usable_designs[0];
        log("单设计代理路径直接采用 integrated 最终方案，跳过 synthesize agent。");
    } else {
        let synthesis_prompt = "你是代码拆分主方案综合代理，使用与主会话相同模型。只读，不得改文件。\n";
        synthesis_prompt += "计划：`" + plan_path + "`；源文件：`" + source_file + "`。\n";
        synthesis_prompt += "plan 声明风险：" + risk_label + "。\n";
        synthesis_prompt += "互补职责设计结果：" + json_encode(usable_designs) + "\n";
        synthesis_prompt += "这些结果按角色刻意不重复。请再次抽查真实代码，合并互补交付、处理 cross_scope_blockers、解决分歧并补齐角色间空隙，输出唯一可执行方案。\n";
        synthesis_prompt += "implementation_plan 必须足够详细，供冷启动 lightweight 实现代理直接执行，但不要把多个角色的原文机械拼接。\n";
        synthesis_prompt += "强制保持公开 API/re-export、DTO、事务边界、幂等语义、审计顺序、外部调用位置、测试行为；私有 helper 跟随唯一调用方；不得机械按行数切割。\n";
        synthesis_prompt += "verification_commands 合并 plan 与仓库门禁，并优先使用只读或限定 crate/文件范围的命令；会改写无关文件或生成物的命令必须改用不会写工作区的等价检查，或明确记录为阻塞，不得通过更换 Cargo target 目录规避。original_tests 必须覆盖拆分前已有测试。";
        synthesis_prompt += cargo_target_policy;
        let synthesis = agent(synthesis_prompt, #{
            label: "synthesize:" + plan_index.to_string(),
            capability_mode: "read-only",
            output_schema: synthesis_schema,
        });
        if synthesis == () || !synthesis.success {
            processed = add_result(processed, plan_path, "blocked", "主方案综合失败");
            blocked_count += 1;
            plan_index += 1;
            continue;
        }
        final_plan = synthesis.output;
    }

    if commands_use_forbidden_cargo_target(final_plan.verification_commands)
        || commands_use_forbidden_cargo_target(final_plan.original_tests) {
        processed = add_result(processed, plan_path, "blocked", "最终方案包含临时或自定义 Cargo target 目录命令");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    // =================================================================
    // Phase 3: lightweight implementation in shared workspace
    // =================================================================
    phase("Implement");

    let implementation_prompt = "你是唯一代码实现代理，类型为 lightweight。直接在当前共享 workspace 实现拆分。\n";
    implementation_prompt += "计划文件：`" + plan_path + "`；源文件：`" + source_file + "`。\n";
    implementation_prompt += "最终实施方案 JSON：" + json_encode(final_plan) + "\n";
    implementation_prompt += "plan 声明风险：" + risk_label + "。\n";
    implementation_prompt += "执行要求：\n";
    implementation_prompt += "1. 开始前读取 git status/diff，保留用户已有修改；只改本计划要求的源码、必要模块入口和测试。\n";
    implementation_prompt += "2. 严格执行纯代码搬迁，不改变公开 API、DTO、业务逻辑、事务/幂等/审计/外部调用顺序。\n";
    implementation_prompt += "3. 使用最小可见性；私有 helper 与唯一调用方同文件；保持所有原方法文档注释。\n";
    implementation_prompt += "4. 如果当前代码已完全符合方案，status=no_changes，不制造无意义 diff，但仍运行验证。\n";
    implementation_prompt += "5. 由你独立承担全部编译质量门禁：真实运行 final_plan.verification_commands 与 final_plan.original_tests，必须覆盖适用的格式检查、cargo check、cargo clippy 和测试；每条可能格式化或生成文件的命令前后都检查 git status/diff。优先使用 --check 或限定到本次 crate/文件，禁止用全仓修改命令顺手改写计划外文件；会触发生成物的 workspace 命令只能使用不会写工作区的等价门禁，否则记录阻塞。若被计划外既有问题阻断，记录精确文件和错误，不得修改无关文件。\n";
    implementation_prompt += "6. verification_passed 只有在所有要求的编译、Clippy 和测试命令都真实成功时才能为 true；任一命令失败、未运行或无法运行都必须 status=blocked、verification_passed=false。后续 Review 不会替你补跑任何 Cargo 命令。\n";
    implementation_prompt += "7. 不要编辑 plan.md 的已解决状态；只有独立 Review 通过后才会标记。\n";
    implementation_prompt += "8. 最终按 schema 返回修改文件、完整命令和测试结果。";
    implementation_prompt += cargo_target_policy;

    let implementation = agent(implementation_prompt, #{
        label: "implement:" + plan_index.to_string(),
        capability_mode: "all",
        agent_type: "lightweight",
        output_schema: implementation_schema,
    });
    if implementation == () || !implementation.success || implementation.output.status == "blocked"
        || implementation.output.verification_passed != true {
        let impl_reason = "lightweight 实现失败，或编译/Clippy/测试未全部通过";
        if implementation != () && implementation.success && implementation.output.blocker != () {
            impl_reason = implementation.output.blocker;
        }
        processed = add_result(processed, plan_path, "blocked", impl_reason);
        blocked_count += 1;
        plan_index += 1;
        continue;
    }
    if commands_use_forbidden_cargo_target(implementation.output.commands) {
        processed = add_result(processed, plan_path, "blocked", "实现代理报告了临时或自定义 Cargo target 目录命令");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    let current_impl = implementation.output;
    let implementation_agent_id = implementation.agent_id;

    // =================================================================
    // Phase 4: independent same-model review; optional lightweight fixes
    // =================================================================
    phase("Review");

    let review_round = 0;
    let review_passed = false;
    let last_review = ();
    let last_fix = ();
    let next_review_mode = "full";
    while review_round <= max_fix_rounds && !review_passed {
        let review_prompt = "你是独立纯代码逻辑 Review 子代理，使用与主会话相同模型。只读审查，禁止修改文件。严禁执行任何 Cargo 命令，包括 cargo fmt、cargo check、cargo clippy、cargo test、cargo build 及其任何变体；编译、Clippy 和测试已由 Implementation/Fix 代理负责并由 workflow 前置门禁。\n";
        review_prompt += "计划：`" + plan_path + "`；源文件：`" + source_file + "`。\n";
        review_prompt += "最终方案 JSON：" + json_encode(final_plan) + "\n";
        review_prompt += "当前实现/Fix 报告 JSON：" + json_encode(current_impl) + "\n";

        if next_review_mode == "delta" && last_review != () && last_fix != () {
            review_prompt += "本轮先执行增量复核。上轮完整 Review JSON：" + json_encode(last_review) + "\n";
            review_prompt += "Fix JSON：" + json_encode(last_fix) + "\n";
            review_prompt += "先独立执行 git status/diff，并按 final_plan.expected_files 排序计算目标快照指纹：逐个记录路径、文件内容 SHA-256；缺失文件记录 ABSENT。\n";
            review_prompt += "只有目标快照与上轮 Review 及 Fix 报告完全一致、Fix 明确 cleanup_only=true 且 target_files_changed=false 时，才可保持 review_mode=delta。\n";
            review_prompt += "增量复核必须确认上轮所有 blocking scope_drift findings 已解决、没有新增计划外变更、git diff --check 通过。可复用上轮完整 API/行为分析证据，不必重新逐符号审查；严禁运行任何编译、Clippy 或测试命令。\n";
            review_prompt += "如果目标快照不一致、Fix 触及 expected_files、无法证明只是清理工作区副作用，必须在同一轮自动扩展为完整 Review，设置 review_mode=full，并执行下面完整审查要求后再给结论。\n";
        } else {
            review_prompt += "本轮必须执行完整 Review，review_mode=full。\n";
        }

        review_prompt += "当本轮是 full，或 delta 因证据不足自动升级为 full 时：使用 git status、git diff、read_file/grep 检查符号遗漏/重复、文件职责、imports/trait scope、visibility/re-export/API、私有 helper、循环依赖、事务/幂等/审计/外部调用和写入顺序、错误语义、测试代码迁移、生成物漂移及计划外修改。只做静态代码与 diff 逻辑审查。\n";
        review_prompt += "严禁执行 final_plan.original_tests、final_plan.verification_commands 或任何 cargo/rustc 编译、Clippy、测试、构建、格式化命令；也不得以验证为由生成或修改文件。Implementation/Fix 报告中的 verification_passed 已由 workflow 校验，Review 不得重复验证或推翻该编译门禁。\n";
        review_prompt += "每个 finding 必须分类：implementation、test、scope_drift 或 unknown。测试类 finding 只能来自测试代码的静态逻辑审查，不得通过运行测试产生。\n";
        review_prompt += "无论 full 或 delta，都必须返回非空 target_snapshot_fingerprint。pass 只有在 plan_match=true、drift=false、api_preserved=true、behavior_preserved=true 且没有 blocking finding 时才可为 true。\n";
        review_prompt += "evidence 必须说明本轮是 full 还是 delta、目标快照计算方式、读过的关键路径、复核的 API/事务/行为边界和具体代码依据；不得包含声称由 Review 执行的 Cargo 命令或测试结果，无代码证据不得通过。";
        review_prompt += cargo_target_policy;

        let review = agent(review_prompt, #{
            label: "review:" + plan_index.to_string() + ":" + review_round.to_string() + ":" + next_review_mode,
            capability_mode: "read-only",
            output_schema: review_schema,
        });
        if review != () && review.success {
            last_review = review.output;
            if review.output.pass == true
                && review.output.plan_match == true
                && review.output.drift == false
                && review.output.api_preserved == true
                && review.output.behavior_preserved == true
                && review.output.target_snapshot_fingerprint != "" {
                review_passed = true;
            }
        }

        if !review_passed && review_round < max_fix_rounds {
            if last_review == () {
                log("Review 未返回可用结果，无法安全生成修复范围，本计划停止自动 Fix。");
                review_round = max_fix_rounds;
            } else {
                log("Review 未通过，启动 lightweight 修复轮次 " + (review_round + 1).to_string() + "：" + json_encode(last_review.findings));

                let has_blocking = false;
                let scope_only_candidate = true;
                let finding_i = 0;
                while finding_i < last_review.findings.len() {
                    let finding = last_review.findings[finding_i];
                    if finding.severity == "blocking" {
                        has_blocking = true;
                        if finding.category != "scope_drift" {
                            scope_only_candidate = false;
                        }
                    }
                    finding_i += 1;
                }
                if !has_blocking
                    || last_review.plan_match != true
                    || last_review.api_preserved != true
                    || last_review.behavior_preserved != true {
                    scope_only_candidate = false;
                }

                let fix_prompt = "继续处理同一个拆分任务。独立 Review 未通过。\n";
                fix_prompt += "最终方案 JSON：" + json_encode(final_plan) + "\n";
                fix_prompt += "Review 结果 JSON：" + json_encode(last_review) + "\n";
                fix_prompt += "scope_only_candidate=" + scope_only_candidate.to_string() + "。只修复 blocking findings 和由本 workflow 引入的问题，不得修改计划外既有问题或改变业务语义。\n";
                fix_prompt += "修复前后都按 final_plan.expected_files 排序计算目标快照指纹：逐个记录路径、文件内容 SHA-256；缺失文件记录 ABSENT。禁止盲目 checkout/reset 覆盖可能属于用户或并行任务的修改；只有在有证据证明差异由本 workflow 命令产生且起始基线无该差异时才可安全清理，否则 status=blocked。\n";
                fix_prompt += "cleanup_only 仅在所有修复都只是清理 expected_files 外的 workflow 副作用、且目标快照与 Review 完全一致时为 true；任何 expected_files 内容或存在状态变化都必须 target_files_changed=true。\n";
                fix_prompt += "如果修改了目标源码，由你重新运行完整的格式检查、cargo check、cargo clippy、原始相关测试和适用门禁；如果只是 cleanup_only 且 target_files_changed=false，可继承上轮已通过的编译验证，但仍需运行 git diff --check 和计划外文件零差异检查。verification_passed 只有在目标代码当前已具备有效的编译、Clippy、测试通过证据时才能为 true；否则 status=blocked。后续 Review 严禁补跑 Cargo。不要标记 plan 已解决，按 fix schema 返回。";
                fix_prompt += cargo_target_policy;
                let fixed = agent(fix_prompt, #{
                    label: "fix:" + plan_index.to_string() + ":" + review_round.to_string(),
                    capability_mode: "all",
                    agent_type: "lightweight",
                    resume_from: implementation_agent_id,
                    output_schema: fix_schema,
                });
                if fixed != () && fixed.success && fixed.output.status != "blocked"
                    && fixed.output.verification_passed == true
                    && !commands_use_forbidden_cargo_target(fixed.output.commands) {
                    last_fix = fixed.output;
                    current_impl = fixed.output;
                    implementation_agent_id = fixed.agent_id;
                    next_review_mode = "full";
                    if scope_only_candidate
                        && fixed.output.cleanup_only == true
                        && fixed.output.target_files_changed == false
                        && fixed.output.target_snapshot_fingerprint == last_review.target_snapshot_fingerprint {
                        next_review_mode = "delta";
                        log("Fix 仅清理计划外副作用且目标快照未变，下一轮执行独立增量复核。若指纹不一致，Reviewer 必须自动升级为完整 Review。");
                    } else {
                        log("Fix 触及目标实现或无法证明快照未变，下一轮执行完整 Review。");
                    }
                } else {
                    log("lightweight Fix 失败、被阻塞或报告了临时/自定义 Cargo target 目录命令，本计划停止自动修复。");
                    review_round = max_fix_rounds;
                }
            }
        }
        review_round += 1;
    }

    if !review_passed {
        let review_reason = "Review 未通过，未标记已解决";
        if last_review != () {
            review_reason += "：" + json_encode(last_review.findings);
        }
        processed = add_result(processed, plan_path, "blocked", review_reason);
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    // =================================================================
    // Phase 5: mark plan resolved only after independent review + tests
    // =================================================================
    phase("Resolve");

    let marker_prompt = "只编辑计划文件 `" + plan_path + "`，不要修改任何源码。\n";
    marker_prompt += "Implementation/Fix 已报告编译、Clippy 和原始相关测试全部通过，workflow 已完成 verification_passed 门禁；独立 Review 已确认实现与方案匹配且无行为/API 漂移。\n";
    marker_prompt += "若文件末尾尚无 `## 处理状态`，追加以下内容；若已有该节则幂等更新，不得重复：\n\n";
    marker_prompt += "## 处理状态\n\n- 状态：**已解决**\n- 验证：实现与拆分方案匹配，原始相关测试通过，独立 Review 通过。\n\n";
    marker_prompt += "使用 read_file 与 search_replace/write 完成，并重新读取确认。按 schema 返回 marked、plan、summary。";
    marker_prompt += cargo_target_policy;
    let marker = agent(marker_prompt, #{
        label: "resolve:" + plan_index.to_string(),
        capability_mode: "read-write",
        agent_type: "lightweight",
        output_schema: marker_schema,
    });
    if marker == () || !marker.success || marker.output.marked == false {
        processed = add_result(processed, plan_path, "blocked", "实现和 Review 已通过，但 plan.md 标记已解决失败");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    processed = add_result(processed, plan_path, "resolved", marker.output.summary);
    resolved_count += 1;
    log("已解决并标记：" + plan_path);
    plan_index += 1;
}

let report = "# backend 拆分计划执行结果\n\n";
report += "- 计划根目录：`" + plan_root + "`\n";
report += "- 本次处理：" + processed.len().to_string() + "\n";
report += "- 已解决：" + resolved_count.to_string() + "\n";
report += "- 阻塞：" + blocked_count.to_string() + "\n\n";
report += "## 明细\n\n";
let result_i = 0;
while result_i < processed.len() {
    let item = processed[result_i];
    report += "- `" + item.plan + "` — **" + item.state + "**：" + item.summary + "\n";
    result_i += 1;
}
let report_path = write_scratch_file("backend-split-results.md", report);

complete(#{
    summary: "处理 " + processed.len().to_string() + " 个 backend 拆分计划，已解决 " + resolved_count.to_string() + " 个，阻塞 " + blocked_count.to_string() + " 个。",
    report_path: report_path,
    report: report,
    processed: processed,
    resolved: resolved_count,
    blocked: blocked_count,
});
