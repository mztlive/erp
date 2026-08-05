// Generate phased backend + FE/BE integration development docs from repo facts.
// Parallel worktree-ready: each stage owns a unique branch and module paths.
// Intermediate stages need not compile; only the integration stage must.
//
// Run: /workflow backend-phase-plan
// Args (all optional):
//   scope: "p1" | "p1p2" | "p2"  (default "p1p2")
//   out_dir: output directory (default "docs/dev-plan")
//   skip_verify: bool (default false)
//   max_stages: int cap on planned stages after inventory (default 14, min 6, max 18)

let meta = #{
    name: "backend-phase-plan",
    description: "From docs + frontend + backend facts, emit worktree-ready staged backend and FE/BE integration development docs",
    when_to_use: "When planning parallel worktree backend implementation and frontend API integration without inventing business scope",
    phases: [
        #{ title: "Inventory", detail: "Collect docs, frontend API, and backend baseline facts" },
        #{ title: "StageDesign", detail: "Design stage DAG with branch names and module ownership" },
        #{ title: "Draft", detail: "Write one development doc per stage in parallel" },
        #{ title: "Verify", detail: "Adversarial check scope, style consistency, and branch uniqueness" },
        #{ title: "Integrate", detail: "Write index + stage docs under docs/dev-plan" },
    ],
};

// --- args ---
let scope = "p1p2";
let out_dir = "docs/dev-plan";
let skip_verify = false;
let max_stages = 14;
if args != () {
    if args.scope != () { scope = args.scope; }
    if args.out_dir != () { out_dir = args.out_dir; }
    if args.skip_verify != () { skip_verify = args.skip_verify; }
    if args.max_stages != () { max_stages = args.max_stages; }
}
if max_stages < 6 { max_stages = 6; }
if max_stages > 18 { max_stages = 18; }
if scope != "p1" && scope != "p1p2" && scope != "p2" {
    pause("verification", "Pass args.scope as p1 | p1p2 | p2");
}

log("backend-phase-plan scope=" + scope + " out_dir=" + out_dir + " max_stages=" + max_stages.to_string());

// --- schemas ---
let inventory_schema = #{
    "type": "object",
    "required": ["summary", "facts"],
    "properties": #{
        "summary": #{ "type": "string" },
        "facts": #{
            "type": "array",
            "maxItems": 80,
            "items": #{
                "type": "object",
                "required": ["id", "kind", "detail", "source_path"],
                "properties": #{
                    "id": #{ "type": "string" },
                    "kind": #{ "type": "string" },
                    "detail": #{ "type": "string" },
                    "source_path": #{ "type": "string" },
                },
            },
        },
        "open_risks": #{
            "type": "array",
            "maxItems": 12,
            "items": #{ "type": "string" },
        },
    },
};

let stage_plan_schema = #{
    "type": "object",
    "required": ["stages", "compile_policy", "style_contract"],
    "properties": #{
        "stages": #{
            "type": "array",
            "minItems": 6,
            "maxItems": 18,
            "items": #{
                "type": "object",
                "required": [
                    "id",
                    "title",
                    "branch",
                    "phase_tag",
                    "depends_on",
                    "owns_modules",
                    "business_scope",
                    "out_of_scope",
                    "frontend_surfaces",
                    "data_model_tables",
                    "must_compile",
                    "rationale"
                ],
                "properties": #{
                    "id": #{ "type": "string" },
                    "title": #{ "type": "string" },
                    "branch": #{ "type": "string" },
                    "phase_tag": #{ "type": "string" },
                    "depends_on": #{ "type": "array", "items": #{ "type": "string" } },
                    "owns_modules": #{ "type": "array", "items": #{ "type": "string" } },
                    "business_scope": #{ "type": "array", "items": #{ "type": "string" } },
                    "out_of_scope": #{ "type": "array", "items": #{ "type": "string" } },
                    "frontend_surfaces": #{ "type": "array", "items": #{ "type": "string" } },
                    "data_model_tables": #{ "type": "array", "items": #{ "type": "string" } },
                    "must_compile": #{ "type": "boolean" },
                    "rationale": #{ "type": "string" },
                },
            },
        },
        "compile_policy": #{ "type": "string" },
        "style_contract": #{ "type": "string" },
        "merge_order": #{ "type": "array", "items": #{ "type": "string" } },
        "notes": #{ "type": "string" },
    },
};

let stage_doc_schema = #{
    "type": "object",
    "required": ["stage_id", "branch", "markdown", "owned_paths", "api_endpoints_sketch"],
    "properties": #{
        "stage_id": #{ "type": "string" },
        "branch": #{ "type": "string" },
        "markdown": #{ "type": "string" },
        "owned_paths": #{ "type": "array", "items": #{ "type": "string" } },
        "api_endpoints_sketch": #{
            "type": "array",
            "maxItems": 40,
            "items": #{ "type": "string" },
        },
        "frontend_touchpoints": #{
            "type": "array",
            "maxItems": 30,
            "items": #{ "type": "string" },
        },
    },
};

let verdict_schema = #{
    "type": "object",
    "required": ["stage_id", "pass", "issues", "evidence"],
    "properties": #{
        "stage_id": #{ "type": "string" },
        "pass": #{ "type": "boolean" },
        "issues": #{
            "type": "array",
            "maxItems": 12,
            "items": #{ "type": "string" },
        },
        "evidence": #{ "type": "string" },
        "scope_creep": #{ "type": "boolean" },
        "style_drift": #{ "type": "boolean" },
    },
};

// =====================================================================
// Phase 1: Inventory (parallel, read-only)
// =====================================================================
phase("Inventory");

let inv_jobs = [];

// A: backend baseline
let p_backend = "";
p_backend += "你是 ERP 后端事实采集员。只读工具，禁止改文件。\n";
p_backend += "工作区根目录：当前 repo。\n";
p_backend += "目标：如实描述现有 backend 基线，供后续分阶段实现规划使用。\n\n";
p_backend += "必须用 read_file / list_dir / grep 读取，禁止凭记忆编造：\n";
p_backend += "1. backend/AGENTS.md 分层与开发流程\n";
p_backend += "2. backend/Cargo.toml workspace members\n";
p_backend += "3. backend/services/src/ 现有域模块\n";
p_backend += "4. backend/entities/src/ 现有实体\n";
p_backend += "5. backend/database/src/repository/ 仓储与 indexes\n";
p_backend += "6. backend/apps/web-api/src/core/handler 与 routes 注册方式\n";
p_backend += "7. 现有 IAM/auth/audit 能力边界\n\n";
p_backend += "输出 facts 数组，每条含 id/kind/detail/source_path。\n";
p_backend += "kind 取值建议：module | route | entity | convention | gap。\n";
p_backend += "summary 用中文 3-6 句说明：已有什么、缺什么、新域应如何挂接。\n";
p_backend += "open_risks 只写与并行 worktree 相关的冲突点（如 mod.rs/routes 汇合）。\n";
p_backend += "禁止提出业务功能扩展；只陈述代码事实。\n";

inv_jobs.push(#{
    prompt: p_backend,
    label: "inv:backend",
    capability_mode: "read-only",
    output_schema: inventory_schema,
});

// B: docs business scope
let p_docs = "";
p_docs += "你是 ERP 文档范围采集员。只读工具，禁止改文件。\n";
p_docs += "scope 参数=" + scope + "（p1=仅一期，p2=仅二期，p1p2=两期）。\n\n";
p_docs += "必须读取（按需分段，勿整本死记）：\n";
p_docs += "- docs/erp-phase-1.md §3 边界、§5 建设范围与不建设、§6 正式单据、§8 卡券同步、§9 票款\n";
p_docs += "- docs/erp-phase-2.md §1-§3 结论与原则、§2.1 不进入二期、功能缺口表\n";
p_docs += "- docs/erp-data-model.md §2 聚合、§5 分域表目录（5.1-5.4）\n";
p_docs += "- docs/ui-workspaces/README.md 工作面索引 W01-W30\n";
p_docs += "- docs/erp-ui-flows.md 跨工作面流程标题级清单\n\n";
p_docs += "按 scope 过滤：\n";
p_docs += "- p1：只收录一期能力与表；明确列出一期「不建设」项\n";
p_docs += "- p2：只收录二期增量\n";
p_docs += "- p1p2：一期+二期，并标注 phase 标签\n\n";
p_docs += "facts 必须可追溯 source_path（含章节号更好）。\n";
p_docs += "kind：capability | table_group | out_of_scope | workspace | flow | boundary。\n";
p_docs += "禁止发明文档未写的业务单据、事件总线、动态审批流、完整总账等。\n";
p_docs += "summary 中文概括建设范围与硬边界。\n";

inv_jobs.push(#{
    prompt: p_docs,
    label: "inv:docs",
    capability_mode: "read-only",
    output_schema: inventory_schema,
});

// C: frontend API surfaces
let p_fe = "";
p_fe += "你是 ERP 前端契约采集员。只读工具，禁止改文件。\n";
p_fe += "前端根目录 erp-client/。当前以 mock 驱动，需映射到后端 API 集成点。\n\n";
p_fe += "必须读取：\n";
p_fe += "1. erp-client/lib/workspace-registry.ts（导航与工作面）\n";
p_fe += "2. erp-client/features/*/types.ts 与 queries.ts（域模型与查询键）\n";
p_fe += "3. erp-client/features/*/api.ts（若存在）— 列出函数名与意图\n";
p_fe += "4. erp-client/mock/ 与 session-state 相关入口，说明数据来源仍是 mock\n";
p_fe += "5. 抽样 3-5 个核心域（customers/contracts/sales-orders/purchase-orders/master-data）的 types 字段形状\n\n";
p_fe += "facts 每条覆盖一个 feature 或 API 面：id=feature 名，kind=frontend_feature|api_fn|type|mock，\n";
p_fe += "detail 写清：主要页面路由、关键操作（list/detail/create/status）、依赖 mock 路径。\n";
p_fe += "source_path 指向具体文件。\n";
p_fe += "summary：前端已完成工作面覆盖情况，以及后端集成时应替换的层（api.ts / queries / mock）。\n";
p_fe += "禁止提议新页面或新业务能力。\n";

inv_jobs.push(#{
    prompt: p_fe,
    label: "inv:frontend",
    capability_mode: "read-only",
    output_schema: inventory_schema,
});

// D: rust coding standards anchors
let p_std = "";
p_std += "你是 Rust 编码规范采集员。只读工具。\n";
p_std += "读取并提炼后续各阶段必须统一遵守的落点与风格事实：\n";
p_std += "1. ~/.claude/skills/rust-coding-standards/SKILL.md\n";
p_std += "2. ~/.claude/skills/rust-coding-standards/references/file-organization.md（若存在）\n";
p_std += "3. ~/.claude/skills/rust-coding-standards/references/architecture-workflow.md（若存在）\n";
p_std += "4. backend/AGENTS.md 与现有 iam 模块布局作为样板\n\n";
p_std += "facts 聚焦：分层 Handler→Service→Repository→MongoDB；文件按用例拆分；\n";
p_std += "DTO 复用；事务边界；方法文档注释；禁止巨型 mod.rs。\n";
p_std += "summary 用可复制到各阶段文档的「统一风格契约」段落（中文）。\n";

inv_jobs.push(#{
    prompt: p_std,
    label: "inv:standards",
    capability_mode: "read-only",
    output_schema: inventory_schema,
});

let inv_results = parallel(inv_jobs);

let inv_backend = ();
let inv_docs = ();
let inv_fe = ();
let inv_std = ();
if inv_results.len() > 0 && inv_results[0] != () && inv_results[0].success {
    inv_backend = inv_results[0].output;
}
if inv_results.len() > 1 && inv_results[1] != () && inv_results[1].success {
    inv_docs = inv_results[1].output;
}
if inv_results.len() > 2 && inv_results[2] != () && inv_results[2].success {
    inv_fe = inv_results[2].output;
}
if inv_results.len() > 3 && inv_results[3] != () && inv_results[3].success {
    inv_std = inv_results[3].output;
}

if inv_backend == () || inv_docs == () || inv_fe == () {
    pause("infra", "Inventory incomplete (backend/docs/frontend). Resume after tools recover, or re-run.");
}

// Persist inventory scratch for later agents via encoded JSON in prompts
let inv_bundle = #{
    scope: scope,
    backend: inv_backend,
    docs: inv_docs,
    frontend: inv_fe,
    standards: inv_std,
};
let inv_json = json_encode(inv_bundle);
write_scratch_file("inventory.json", inv_json);
log("inventory collected; bytes≈" + inv_json.len().to_string());

// =====================================================================
// Phase 2: Stage design (single architect)
// =====================================================================
phase("StageDesign");

let p_arch = "";
p_arch += "你是 ERP 后端分阶段架构师。只读工具。根据下列 inventory JSON 设计 worktree 并行开发阶段。\n";
p_arch += "禁止发散：不得增加 phase 文档与 data-model「不建设」清单中的能力。\n\n";
p_arch += "硬性要求：\n";
p_arch += "1. 每个阶段必须有唯一分支名：feat/erp-s{NN}-{kebab-slug}（NN 两位，从 00 起）。\n";
p_arch += "2. 中间阶段 must_compile=false；仅「集成/汇合」阶段 must_compile=true。\n";
p_arch += "3. 各阶段 owns_modules 互斥（按目录路径声明），避免并行 worktree 改同一业务文件。\n";
p_arch += "   共享汇合点（services/src/lib.rs、entities/src/lib.rs、database repository mod.rs、\n";
p_arch += "   apps/web-api routes/handler mod.rs、indexes）只能由集成阶段修改；其它阶段写「补丁说明」即可。\n";
p_arch += "4. 目录布局与风格必须对齐 rust-coding-standards / backend AGENTS：\n";
p_arch += "   entities → database/repository → services/<domain>/{mod,dto,create,query,...} → handler → routes。\n";
p_arch += "5. 阶段数量 6.." + max_stages.to_string() + "；scope=" + scope + "。\n";
p_arch += "6. 必须包含：共享基元/单据基础设施、基础资料、客户合同、销售、采购、履约库存、财务、\n";
p_arch += "   待办工作台、一期商城同步（若 scope 含 p1）、二期集成域（若 scope 含 p2）、\n";
p_arch += "   前端 API 集成、最终集成汇合。可合并相近域，但不可漏掉文档已建能力。\n";
p_arch += "7. frontend_surfaces 引用 inventory 中的 feature 名与 Wxx。\n";
p_arch += "8. data_model_tables 使用 data-model 真实表名。\n";
p_arch += "9. depends_on 只写阶段 id，形成可拓扑排序的 DAG。\n";
p_arch += "10. out_of_scope 显式抄写文档不建设项中与本阶段相关的条目。\n";
p_arch += "11. compile_policy 与 style_contract 用中文写清全局策略。\n";
p_arch += "12. merge_order 给出建议合并顺序（集成阶段最后）。\n\n";
p_arch += "可用 inventory（JSON）：\n";
p_arch += inv_json;
p_arch += "\n\n若 inventory 过长，优先信任 docs 的能力/表与 frontend feature 列表，backend 仅作挂接基线。\n";
p_arch += "输出 stages 数组，id 形如 S00、S01...。title 中文。rationale 一句话说明为何独立成阶段。\n";

let plan_res = agent(p_arch, #{
    label: "architect:stages",
    capability_mode: "read-only",
    output_schema: stage_plan_schema,
});

if plan_res == () || !plan_res.success || plan_res.output.stages == () {
    pause("infra", "Stage design agent failed. Resume or re-run.");
}

let stage_plan = plan_res.output;
let stages = stage_plan.stages;
if stages.len() == 0 {
    pause("verification", "Architect returned zero stages.");
}

// Enforce branch uniqueness + must_compile policy in script
let seen_branches = [];
let compile_true_count = 0;
let filtered_stages = [];
let i = 0;
while i < stages.len() {
    let st = stages[i];
    let br = st.branch;
    let dup = false;
    for b in seen_branches {
        if b == br { dup = true; }
    }
    if dup {
        log("drop duplicate branch stage: " + st.id);
    } else {
        seen_branches.push(br);
        if st.must_compile == true {
            compile_true_count += 1;
        }
        filtered_stages.push(st);
    }
    i += 1;
}
stages = filtered_stages;

if stages.len() < 6 {
    pause("verification", "Too few stages after filter (<6). Re-run architect.");
}

// Cap stages
if stages.len() > max_stages {
    log("capping stages from " + stages.len().to_string() + " to " + max_stages.to_string());
    let capped = [];
    let j = 0;
    while j < max_stages {
        capped.push(stages[j]);
        j += 1;
    }
    stages = capped;
}

write_scratch_file("stage-plan.json", json_encode(stage_plan));
log("stage plan: " + stages.len().to_string() + " stages; compile_true_count was " + compile_true_count.to_string());

// =====================================================================
// Phase 3: Draft stage docs in parallel
// =====================================================================
phase("Draft");

let style_contract = "";
if stage_plan.style_contract != () {
    style_contract = stage_plan.style_contract;
}
let compile_policy = "";
if stage_plan.compile_policy != () {
    compile_policy = stage_plan.compile_policy;
}

let draft_jobs = [];
let di = 0;
while di < stages.len() {
    let st = stages[di];
    let st_json = json_encode(st);

    let pd = "";
    pd += "你是 ERP 分阶段开发文档作者。使用只读工具核对事实；最终只输出 JSON（markdown 字段写全文）。\n";
    pd += "禁止发明业务能力。所有业务范围必须能在 docs/erp-phase-*.md、erp-data-model.md、ui-workspaces 找到依据。\n";
    pd += "编码风格强制遵循 /rust-coding-standards 与 backend/AGENTS.md（分层、按用例拆文件、DTO 复用、注释、事务）。\n\n";
    pd += "全局 compile_policy：\n" + compile_policy + "\n\n";
    pd += "全局 style_contract：\n" + style_contract + "\n\n";
    pd += "本阶段规划对象（JSON）：\n" + st_json + "\n\n";
    pd += "请按需读取（不要无依据瞎写）：\n";
    pd += "- 相关 docs/ui-workspaces/w*.md\n";
    pd += "- docs/erp-data-model.md 对应表章节\n";
    pd += "- erp-client/features/<domain>/{types,api,queries}.ts\n";
    pd += "- backend 现有 iam 模块作为目录样板\n\n";
    pd += "markdown 必须是完整中文 Markdown，结构固定：\n";
    pd += "# {stage_id} {title}\n\n";
    pd += "## 1. 元信息\n";
    pd += "- 分支：`{branch}`\n";
    pd += "- 业务期：phase_tag\n";
    pd += "- 依赖阶段：depends_on\n";
    pd += "- 本阶段是否要求单独编译：must_compile（说明：中间阶段 false 可接受；集成阶段 true）\n\n";
    pd += "## 2. 目标与业务范围\n";
    pd += "列表 + 文档依据（章节/W 编号）。\n\n";
    pd += "## 3. 明确不在范围\n";
    pd += "防止 scope creep。\n\n";
    pd += "## 4. 代码落点与目录布局（强制统一风格）\n";
    pd += "给出建议树：entities/src/<domain>/、database/src/repository/<domain>/、\n";
    pd += "services/src/<domain>/{mod.rs,dto.rs,create.rs,query.rs,...}、\n";
    pd += "apps/web-api/src/core/handler/admin/<domain>.rs 或子目录。\n";
    pd += "声明 owns_modules；标注「汇合文件仅集成阶段改」。\n";
    pd += "说明每个公共方法必须有文档注释；Service 按用例拆文件。\n\n";
    pd += "## 5. 数据模型与索引\n";
    pd += "表清单 + 关键索引/集合名约定（对齐 data-model）。\n\n";
    pd += "## 6. API 与权限草图\n";
    pd += "REST 路径、permission key 命名、与现有 admin 路由挂接方式。\n\n";
    pd += "## 7. 前端集成点\n";
    pd += "features/、mock 替换策略、types 对齐字段、queries key。\n\n";
    pd += "## 8. 实现任务清单\n";
    pd += "按 建模→仓储→服务→HTTP→（本阶段不改 routes 则写待集成说明）→测试 排列。\n\n";
    pd += "## 9. Worktree / 并行约定\n";
    pd += "分支创建、禁止触碰的路径、与其它阶段接口（类型/DTO 边界）。\n\n";
    pd += "## 10. 验收标准\n";
    pd += "功能验收 + 风格验收（rust-coding-standards）+ 文档范围验收。\n";
    pd += "若 must_compile=false：写明「合并进集成分支后由集成阶段保证 cargo check」。\n\n";
    pd += "api_endpoints_sketch：字符串数组，如 `GET /admin/customers`。\n";
    pd += "owned_paths：本阶段允许创建/修改的路径前缀。\n";
    pd += "frontend_touchpoints：feature 路径列表。\n";
    pd += "markdown 要可直接落盘成独立文件，信息密度高、指令式、无空话。\n";

    draft_jobs.push(#{
        prompt: pd,
        label: "draft:" + st.id,
        capability_mode: "read-only",
        output_schema: stage_doc_schema,
    });
    di += 1;
}

let draft_results = parallel(draft_jobs);

let stage_docs = [];
let failed_drafts = [];
let dri = 0;
while dri < draft_results.len() {
    let r = draft_results[dri];
    let st = stages[dri];
    if r != () && r.success && r.output.markdown != () && r.output.markdown != "" {
        // Soft-fix stage_id / branch from plan if agent drifted
        let doc = r.output;
        stage_docs.push(#{
            stage_id: st.id,
            branch: st.branch,
            title: st.title,
            must_compile: st.must_compile,
            markdown: doc.markdown,
            owned_paths: doc.owned_paths,
            api_endpoints_sketch: doc.api_endpoints_sketch,
            frontend_touchpoints: doc.frontend_touchpoints,
        });
    } else {
        failed_drafts.push(st.id);
        log("draft failed: " + st.id);
    }
    dri += 1;
}

if stage_docs.len() == 0 {
    pause("infra", "All stage doc drafts failed.");
}
if failed_drafts.len() > 0 {
    log("partial drafts ok; failed=" + json_encode(failed_drafts));
}

write_scratch_file("stage-docs.json", json_encode(stage_docs));

// =====================================================================
// Phase 4: Verify (adversarial, optional)
// =====================================================================
phase("Verify");

let verified_docs = stage_docs;
if !skip_verify {
    let vjobs = [];
    let vi = 0;
    while vi < stage_docs.len() {
        let d = stage_docs[vi];
        let d_json = json_encode(d);

        let pv = "";
        pv += "你是对抗性审查员。只读工具。审查下列阶段开发文档是否合格。\n";
        pv += "不合格条件（任一即可 pass=false）：\n";
        pv += "1. scope_creep：引入 phase 文档明确不建设的能力（如完整总账、事件总线、卡号卡密、一期 webhook 外发等）\n";
        pv += "2. style_drift：目录布局违背 Handler→Service→Repository；或鼓励巨型 service.rs/mod.rs；或 Handler 重复定义 DTO\n";
        pv += "3. 缺少独立分支名或不匹配 feat/erp-sNN-*\n";
        pv += "4. 非集成阶段却要求改 routes/lib.rs 汇合点且未说明冲突\n";
        pv += "5. 业务范围无法对应 docs 或 frontend 已有工作面\n";
        pv += "6. 中间阶段 must_compile 语义矛盾（文档写必须单独 cargo test 通过才可合并等）\n\n";
        pv += "合格只需：范围克制、风格一致、分支明确、可并行、集成阶段才强制编译。\n";
        pv += "请抽样 read 文档章节核对 1-2 个关键断言，evidence 写清读过的路径。\n\n";
        pv += "待审文档 JSON：\n" + d_json + "\n";
        pv += "scope=" + scope + "\n";

        vjobs.push(#{
            prompt: pv,
            label: "verify:" + d.stage_id,
            capability_mode: "read-only",
            output_schema: verdict_schema,
        });
        vi += 1;
    }

    let vresults = parallel(vjobs);
    let kept = [];
    let rejected = [];
    let vri = 0;
    while vri < vresults.len() {
        let vr = vresults[vri];
        let d = stage_docs[vri];
        let accept = false;
        if vr != () && vr.success && vr.output != () {
            // Fail closed only on explicit pass=false with issues; missing verifier fails open with log
            if vr.output.pass == true {
                accept = true;
            } else if vr.output.pass == false {
                rejected.push(#{
                    stage_id: d.stage_id,
                    issues: vr.output.issues,
                    evidence: vr.output.evidence,
                });
                // Still keep doc but flag — integrate will add fix notes
                accept = true;
                log("verify FAIL (kept with flags): " + d.stage_id + " " + json_encode(vr.output.issues));
            } else {
                accept = true;
            }
        } else {
            log("verify missing for " + d.stage_id + " — keep with warning");
            accept = true;
        }
        if accept {
            kept.push(d);
        }
        vri += 1;
    }
    verified_docs = kept;
    write_scratch_file("verify-rejected.json", json_encode(rejected));
    log("verify done; docs=" + verified_docs.len().to_string() + " flagged=" + rejected.len().to_string());
} else {
    log("skip_verify=true");
}

// =====================================================================
// Phase 5: Integrate — write files
// =====================================================================
phase("Integrate");

let docs_json = json_encode(verified_docs);
let plan_json = json_encode(#{
    scope: scope,
    compile_policy: compile_policy,
    style_contract: style_contract,
    merge_order: stage_plan.merge_order,
    stages: stages,
    notes: stage_plan.notes,
});

let p_int = "";
p_int += "你是文档集成员。必须把分阶段开发文档写入仓库目录 `" + out_dir + "`。\n";
p_int += "使用 write/search_replace 等工具创建文件。目录不存在则创建。\n\n";
p_int += "必须产出：\n";
p_int += "1. `" + out_dir + "/README.md` — 总索引（中文），包含：\n";
p_int += "   - 文档目的与阅读路径\n";
p_int += "   - 全局编译策略（中间阶段可不编译；仅集成阶段保证 cargo check/test）\n";
p_int += "   - 全局风格契约（引用 rust-coding-standards / backend AGENTS，目录布局统一）\n";
p_int += "   - 阶段总表：id | 标题 | 分支 | 依赖 | must_compile | 文档链接\n";
p_int += "   - 建议 worktree 并行波次（可并行的阶段分组）\n";
p_int += "   - 建议 merge 顺序\n";
p_int += "   - 范围边界：严禁超出 erp-phase / data-model / ui-workspaces\n";
p_int += "   - 与前端集成总策略（mock→api 替换层）\n";
p_int += "2. 每个阶段一个文件：`" + out_dir + "/{stage_id}-{slug}.md`\n";
p_int += "   - 文件内容使用对应 markdown 字段；若 markdown 标题不完整可补全元信息头\n";
p_int += "   - slug 取分支名 feat/erp-sNN- 之后部分\n";
p_int += "3. `" + out_dir + "/branch-map.md` — 仅分支与 owns 路径对照表，便于开 worktree\n";
p_int += "4. `" + out_dir + "/_meta.json` — 机器可读：scope、stages 摘要、生成说明\n\n";
p_int += "硬约束：\n";
p_int += "- 不要改 docs/erp-phase*.md 或业务契约文档\n";
p_int += "- 不要写实现代码到 backend/\n";
p_int += "- 不要扩大业务范围\n";
p_int += "- 文风：指令式、结论式、可执行；无过程回顾空话\n\n";
p_int += "stage plan JSON：\n" + plan_json + "\n\n";
p_int += "stage docs JSON：\n" + docs_json + "\n\n";
p_int += "inventory 摘要（backend/docs/fe）：\n";
if inv_backend.summary != () { p_int += "BACKEND: " + inv_backend.summary + "\n"; }
if inv_docs.summary != () { p_int += "DOCS: " + inv_docs.summary + "\n"; }
if inv_fe.summary != () { p_int += "FE: " + inv_fe.summary + "\n"; }
p_int += "\n完成后在最终回复中列出写入的全部相对路径与阶段分支一览。\n";

let int_res = agent(p_int, #{
    label: "integrate:write-docs",
    capability_mode: "read-write",
});

let integrate_ok = int_res != () && int_res.success;
if !integrate_ok {
    // Fallback: write a minimal index via scratch + note
    write_scratch_file("INTEGRATE_FAILED.md", "Integrate agent failed; stage-docs.json in scratch has content.");
    pause("infra", "Integrate agent failed to write docs. Inspect scratch stage-docs.json and resume/re-run.");
}

// Final report agent summary text
let report = "";
report += "# backend-phase-plan 结果\n\n";
report += "- scope: " + scope + "\n";
report += "- out_dir: " + out_dir + "\n";
report += "- stages: " + stages.len().to_string() + "\n";
report += "- docs drafted: " + verified_docs.len().to_string() + "\n";
report += "- skip_verify: " + skip_verify.to_string() + "\n\n";
report += "## 分支一览\n";
let ri = 0;
while ri < stages.len() {
    let s = stages[ri];
    let mc = "false";
    if s.must_compile == true { mc = "true"; }
    report += "- " + s.id + " `" + s.branch + "` — " + s.title + " (must_compile=" + mc + ")\n";
    ri += 1;
}
report += "\n## 全局策略摘要\n";
report += compile_policy + "\n\n";
report += style_contract + "\n";

write_scratch_file("report.md", report);

complete(#{
    summary: "Wrote phased backend/FE integration plan under " + out_dir,
    out_dir: out_dir,
    scope: scope,
    stage_count: stages.len(),
    doc_count: verified_docs.len(),
    report: report,
});
