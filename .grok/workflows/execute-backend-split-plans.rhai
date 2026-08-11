let meta = #{
    name: "execute-backend-split-plans",
    description: "逐个执行 docs/split/backend 拆分计划：动态设计、lightweight 实现、独立 Review、测试通过后标记已解决",
    when_to_use: "需要按 docs/split/backend/**/plan.md 逐个完成 Rust 大文件拆分并验证无方案漂移时",
    phases: [
        #{ title: "Discover", detail: "选择未解决计划并检查工作区安全性" },
        #{ title: "Design", detail: "按风险动态分配同模型设计代理并综合方案" },
        #{ title: "Implement", detail: "由 lightweight 代理在共享工作区实现" },
        #{ title: "Review", detail: "同模型独立审查方案漂移与原始测试" },
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
//   max_designers    - 每个计划最多设计代理数，默认 3，范围 1..3；实际数量由协调代理决定
//   max_fix_rounds   - Review 不通过后的 lightweight 修复轮数，默认 1，范围 0..3
//   allow_dirty      - 是否允许从非干净工作区开始，默认 false；false 时需要用户恢复确认
//
// 模型规则：所有设计、综合和 Review agent 均不指定 model，自动继承主会话模型。
// 代码规则：所有实现与 Review 后修复都显式使用 agent_type="lightweight"。

let discovery_schema = #{
    "type": "object",
    "required": ["plans"],
    "properties": #{
        "plans": #{
            "type": "array",
            "maxItems": 100,
            "items": #{
                "type": "object",
                "required": ["path", "source_file", "reason"],
                "properties": #{
                    "path": #{ "type": "string" },
                    "source_file": #{ "type": "string" },
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

let coordination_schema = #{
    "type": "object",
    "required": [
        "executable", "reason", "source_file", "designer_count", "design_focus",
        "risk", "original_tests", "required_gates"
    ],
    "properties": #{
        "executable": #{ "type": "boolean" },
        "reason": #{ "type": "string" },
        "source_file": #{ "type": "string" },
        "designer_count": #{ "type": "integer", "minimum": 1, "maximum": 3 },
        "design_focus": #{
            "type": "array", "minItems": 1, "maxItems": 3,
            "items": #{ "type": "string" },
        },
        "risk": #{ "type": "string", "enum": ["low", "medium", "high"] },
        "original_tests": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "required_gates": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
    },
};

let design_schema = #{
    "type": "object",
    "required": ["summary", "file_plan", "invariants", "risks", "verification"],
    "properties": #{
        "summary": #{ "type": "string" },
        "file_plan": #{
            "type": "array", "maxItems": 16,
            "items": #{
                "type": "object",
                "required": ["path", "symbols", "visibility_notes"],
                "properties": #{
                    "path": #{ "type": "string" },
                    "symbols": #{ "type": "array", "maxItems": 100, "items": #{ "type": "string" } },
                    "visibility_notes": #{ "type": "string" },
                },
            },
        },
        "invariants": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "risks": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
        "verification": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
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
    "required": ["status", "summary", "changed_files", "commands", "test_results"],
    "properties": #{
        "status": #{ "type": "string", "enum": ["implemented", "no_changes", "blocked"] },
        "summary": #{ "type": "string" },
        "changed_files": #{ "type": "array", "maxItems": 100, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "test_results": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "blocker": #{ "type": "string" },
    },
};

let review_schema = #{
    "type": "object",
    "required": [
        "pass", "plan_match", "drift", "tests_pass", "api_preserved",
        "behavior_preserved", "findings", "test_commands", "evidence"
    ],
    "properties": #{
        "pass": #{ "type": "boolean" },
        "plan_match": #{ "type": "boolean" },
        "drift": #{ "type": "boolean" },
        "tests_pass": #{ "type": "boolean" },
        "api_preserved": #{ "type": "boolean" },
        "behavior_preserved": #{ "type": "boolean" },
        "findings": #{
            "type": "array", "maxItems": 30,
            "items": #{
                "type": "object",
                "required": ["severity", "file", "issue", "required_fix"],
                "properties": #{
                    "severity": #{ "type": "string", "enum": ["blocking", "non_blocking"] },
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                    "required_fix": #{ "type": "string" },
                },
            },
        },
        "test_commands": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "evidence": #{ "type": "string" },
        "quality_gate_blockers": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
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

// ---------- 参数 ----------
let plan_root = "docs/split/backend";
let explicit_plan = ();
let max_plans = 1;
let max_designers = 3;
let max_fix_rounds = 1;
let allow_dirty = false;

if args != () {
    if args.plan_root != () { plan_root = args.plan_root; }
    if args.plan != () { explicit_plan = args.plan; }
    if args.max_plans != () { max_plans = args.max_plans; }
    if args.max_designers != () { max_designers = args.max_designers; }
    if args.max_fix_rounds != () { max_fix_rounds = args.max_fix_rounds; }
    if args.allow_dirty != () { allow_dirty = args.allow_dirty; }
}
if max_plans < 1 { max_plans = 1; }
if max_plans > 5 { max_plans = 5; }
if max_designers < 1 { max_designers = 1; }
if max_designers > 3 { max_designers = 3; }
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
    selected_plans.push(#{ path: explicit_plan, source_file: "", reason: "explicit args.plan" });
} else {
    let discover_prompt = "在仓库根目录枚举 `" + plan_root + "/**/plan.md`，选择尚未解决且具备可执行拆分方案的计划。\n";
    discover_prompt += "必须使用 find/grep/read_file 读取真实文件，不得凭记忆。跳过包含 `- 状态：**已解决**` 的计划；";
    discover_prompt += "跳过拆分结论不是 split、分析失败或仅列入清单但没有具体拆分建议的计划。\n";
    discover_prompt += "从每个计划的“源文件”字段提取 backend/... 相对路径。按以下优先级排序：low 风险 → medium → high；同风险下 M/S → L/XL；再按路径排序。\n";
    discover_prompt += "最多返回 100 个候选，字段为 path、source_file、reason。所有 path 必须位于 `" + plan_root + "`。";
    let discovered = agent(discover_prompt, #{
        label: "discover:unresolved-plans",
        capability_mode: "execute",
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
    // Phase 2: Coordinator decides designer allocation, then same-model fan-out
    // =================================================================
    phase("Design");

    let coordinator_prompt = "你是代码拆分设计协调代理。只读分析 `" + plan_path + "` 及其源文件。\n";
    coordinator_prompt += "必须读取 plan 完整内容、源文件、沿路径生效的 AGENTS.md/Claude.md，以及对应编码规范；禁止修改文件。\n";
    coordinator_prompt += "判断计划是否可执行：源文件存在；计划有具体 split 方案；若代码已经完成该拆分，也可 executable=true，后续将验证并标记。\n";
    coordinator_prompt += "你代表主协调者自行决定需要多少个独立设计代理：低风险且边界清晰用 1 个，中风险/多模块用 2 个，高风险、事务/API/宏/生成逻辑复杂用 3 个；上限 " + max_designers.to_string() + "。\n";
    coordinator_prompt += "design_focus 必须与 designer_count 数量一致，分别覆盖互补视角，例如模块边界、API/可见性、事务/测试/生成物。\n";
    coordinator_prompt += "original_tests 列出现有测试的精确命令或测试模块；required_gates 包含 plan 和仓库要求的格式、编译、Clippy、测试命令。\n";
    coordinator_prompt += "返回 source_file 必须是 backend/ 下真实相对路径。";

    let coordination = agent(coordinator_prompt, #{
        label: "coordinate:" + plan_index.to_string(),
        capability_mode: "read-only",
        output_schema: coordination_schema,
    });
    if coordination == () || !coordination.success || type_of(coordination.output) != "map"
        || coordination.output.executable != true {
        let why = "协调分析失败或计划不可执行";
        if coordination != () && coordination.success && type_of(coordination.output) == "map"
            && coordination.output.reason != () {
            why = coordination.output.reason;
        }
        processed = add_result(processed, plan_path, "blocked", why);
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    let coord = coordination.output;
    if type_of(coord.source_file) != "string" || !coord.source_file.starts_with("backend/")
        || coord.source_file.contains("..") {
        processed = add_result(processed, plan_path, "blocked", "协调代理返回非法 source_file");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    let designer_count = 1;
    if type_of(coord.designer_count) == "i64" { designer_count = coord.designer_count; }
    if designer_count > max_designers { designer_count = max_designers; }
    if designer_count < 1 { designer_count = 1; }
    let risk_label = "unknown";
    if type_of(coord.risk) == "string" { risk_label = coord.risk; }
    log("协调代理按风险 " + risk_label + " 分配 " + designer_count.to_string() + " 个同模型设计代理。");

    let design_jobs = [];
    let designer_i = 0;
    while designer_i < designer_count {
        let focus = "整体拆分正确性与最小行为变化";
        if coord.design_focus != () && designer_i < coord.design_focus.len() {
            focus = coord.design_focus[designer_i];
        }
        let design_prompt = "你是独立代码拆分方案设计代理，使用与主会话相同的模型。只读，不得修改文件。\n";
        design_prompt += "计划：`" + plan_path + "`；源文件：`" + coord.source_file + "`；本代理重点：" + focus + "。\n";
        design_prompt += "必须真实读取完整 plan、完整源文件、相邻模块、调用方、现有测试、AGENTS.md 与适用编码规范。\n";
        design_prompt += "给出逐文件符号归属、imports/trait scope、可见性与 re-export、私有 helper 归属、循环依赖风险、事务/幂等/外部调用不变量、测试迁移与验证命令。\n";
        design_prompt += "目标是纯拆分：不得顺手修改业务规则、错误语义、公开 API、DTO、数据库查询、事务顺序或生成逻辑。\n";
        design_prompt += "若 plan 与当前代码不一致，明确指出应由综合代理采用的修订。空结论只有在读完代码后才允许。";
        design_jobs.push(#{
            prompt: design_prompt,
            label: "design:" + plan_index.to_string() + ":" + designer_i.to_string(),
            capability_mode: "read-only",
            output_schema: design_schema,
        });
        designer_i += 1;
    }

    let design_results = parallel(design_jobs);
    let usable_designs = [];
    let dr_i = 0;
    while dr_i < design_results.len() {
        let dr = design_results[dr_i];
        if dr != () && dr.success && dr.output.summary != () {
            usable_designs.push(dr.output);
        }
        dr_i += 1;
    }
    if usable_designs.len() == 0 {
        processed = add_result(processed, plan_path, "blocked", "所有设计代理均未返回可用方案");
        blocked_count += 1;
        plan_index += 1;
        continue;
    }

    let synthesis_prompt = "你是代码拆分主方案综合代理，使用与主会话相同模型。只读，不得改文件。\n";
    synthesis_prompt += "计划：`" + plan_path + "`；源文件：`" + coord.source_file + "`。\n";
    synthesis_prompt += "协调信息：" + json_encode(coord) + "\n";
    synthesis_prompt += "独立设计结果：" + json_encode(usable_designs) + "\n";
    synthesis_prompt += "请再次抽查真实代码，解决设计分歧并输出唯一可执行方案。implementation_plan 必须足够详细，供冷启动 lightweight 实现代理直接执行。\n";
    synthesis_prompt += "强制保持公开 API/re-export、DTO、事务边界、幂等语义、审计顺序、外部调用位置、测试行为；私有 helper 跟随唯一调用方；不得机械按行数切割。\n";
    synthesis_prompt += "verification_commands 合并 plan 与仓库门禁；original_tests 必须覆盖拆分前已有测试。";
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
    let final_plan = synthesis.output;

    // =================================================================
    // Phase 3: lightweight implementation in shared workspace
    // =================================================================
    phase("Implement");

    let implementation_prompt = "你是唯一代码实现代理，类型为 lightweight。直接在当前共享 workspace 实现拆分。\n";
    implementation_prompt += "计划文件：`" + plan_path + "`；源文件：`" + coord.source_file + "`。\n";
    implementation_prompt += "最终实施方案 JSON：" + json_encode(final_plan) + "\n";
    implementation_prompt += "协调要求 JSON：" + json_encode(coord) + "\n";
    implementation_prompt += "执行要求：\n";
    implementation_prompt += "1. 开始前读取 git status/diff，保留用户已有修改；只改本计划要求的源码、必要模块入口和测试。\n";
    implementation_prompt += "2. 严格执行纯代码搬迁，不改变公开 API、DTO、业务逻辑、事务/幂等/审计/外部调用顺序。\n";
    implementation_prompt += "3. 使用最小可见性；私有 helper 与唯一调用方同文件；保持所有原方法文档注释。\n";
    implementation_prompt += "4. 如果当前代码已完全符合方案，status=no_changes，不制造无意义 diff，但仍运行验证。\n";
    implementation_prompt += "5. 运行 final_plan.verification_commands 与 original_tests 中可执行命令；若 workspace 门禁被计划外既有问题阻断，记录精确文件和错误，不得修改无关文件。\n";
    implementation_prompt += "6. 不要编辑 plan.md 的已解决状态；只有独立 Review 通过后才会标记。\n";
    implementation_prompt += "7. 最终按 schema 返回修改文件、命令和测试结果。";

    let implementation = agent(implementation_prompt, #{
        label: "implement:" + plan_index.to_string(),
        capability_mode: "all",
        agent_type: "lightweight",
        output_schema: implementation_schema,
    });
    if implementation == () || !implementation.success || implementation.output.status == "blocked" {
        let impl_reason = "lightweight 实现失败";
        if implementation != () && implementation.success && implementation.output.blocker != () {
            impl_reason = implementation.output.blocker;
        }
        processed = add_result(processed, plan_path, "blocked", impl_reason);
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
    while review_round <= max_fix_rounds && !review_passed {
        let review_prompt = "你是独立 Review 子代理，使用与主会话相同模型。只读审查，禁止修改文件。\n";
        review_prompt += "计划：`" + plan_path + "`；源文件：`" + coord.source_file + "`。\n";
        review_prompt += "最终方案 JSON：" + json_encode(final_plan) + "\n";
        review_prompt += "实现代理报告 JSON：" + json_encode(current_impl) + "\n";
        review_prompt += "必须真实执行 git status、git diff、read_file/grep，并独立运行 final_plan.original_tests、coord.original_tests 和适用质量门禁。\n";
        review_prompt += "重点检查：符号遗漏/重复；文件职责；imports/trait scope；visibility/re-export/API 兼容；私有 helper；循环依赖；事务、幂等、审计、外部调用和写入顺序；测试迁移；生成物漂移；计划外文件修改。\n";
        review_prompt += "pass 只有在 plan_match=true、drift=false、tests_pass=true、api_preserved=true、behavior_preserved=true 且没有 blocking finding 时才可为 true。\n";
        review_prompt += "tests_pass 要求拆分前已有相关测试真实通过；不能只相信实现代理报告。质量门禁若被 diff 外既有问题阻断，写入 quality_gate_blockers，但仍需明确相关原始测试是否通过。\n";
        review_prompt += "evidence 必须列出读过的关键路径、事务边界位置和真实测试命令结果；无证据不得通过。";

        let review = agent(review_prompt, #{
            label: "review:" + plan_index.to_string() + ":" + review_round.to_string(),
            capability_mode: "execute",
            output_schema: review_schema,
        });
        if review != () && review.success {
            last_review = review.output;
            if review.output.pass == true
                && review.output.plan_match == true
                && review.output.drift == false
                && review.output.tests_pass == true
                && review.output.api_preserved == true
                && review.output.behavior_preserved == true {
                review_passed = true;
            }
        }

        if !review_passed && review_round < max_fix_rounds {
            if last_review == () {
                log("Review 未返回可用结果，启动 lightweight 复查修复轮次 " + (review_round + 1).to_string());
            } else {
                log("Review 未通过，启动 lightweight 修复轮次 " + (review_round + 1).to_string() + "：" + json_encode(last_review.findings));
            }
            let fix_prompt = "继续处理同一个拆分任务。独立 Review 未通过。\n";
            fix_prompt += "Review 结果 JSON：" + json_encode(last_review) + "\n";
            fix_prompt += "只修复 blocking findings 和由本次拆分引入的问题；不得修改计划外既有问题或改变业务语义。\n";
            fix_prompt += "修复后重新运行原始相关测试与方案验证命令，并按 implementation schema 返回。不要标记 plan 已解决。";
            let fixed = agent(fix_prompt, #{
                label: "fix:" + plan_index.to_string() + ":" + review_round.to_string(),
                capability_mode: "all",
                agent_type: "lightweight",
                resume_from: implementation_agent_id,
                output_schema: implementation_schema,
            });
            if fixed != () && fixed.success && fixed.output.status != "blocked" {
                current_impl = fixed.output;
                implementation_agent_id = fixed.agent_id;
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
    marker_prompt += "独立 Review 已确认实现与方案匹配、无行为/API 漂移且原始相关测试通过。\n";
    marker_prompt += "若文件末尾尚无 `## 处理状态`，追加以下内容；若已有该节则幂等更新，不得重复：\n\n";
    marker_prompt += "## 处理状态\n\n- 状态：**已解决**\n- 验证：实现与拆分方案匹配，原始相关测试通过，独立 Review 通过。\n\n";
    marker_prompt += "使用 read_file 与 search_replace/write 完成，并重新读取确认。按 schema 返回 marked、plan、summary。";
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
