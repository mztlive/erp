let meta = #{
    name: "scan-ui-copy",
    description: "扫描 erp-client 全量用户可见文案，识别偏技术化表达并改写为业务语言",
    when_to_use: "当需要对前端所有面向用户的文案做业务化改写、清理技术术语时",
    phases: [
        #{ title: "ScanRewrite", detail: "并行扫描各模块文案并改写为业务语言" },
        #{ title: "Verify", detail: "lint / typecheck / tests / build 门禁" },
        #{ title: "Fix", detail: "针对门禁失败的定向修复" },
    ],
};

let fix_schema = #{
    "type": "object",
    "required": ["summary", "files_scanned", "files_modified", "issues_found", "issues_fixed", "remaining", "details"],
    "properties": #{
        "summary": #{ "type": "string" },
        "files_scanned": #{ "type": "integer" },
        "files_modified": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "issues_found": #{ "type": "integer" },
        "issues_fixed": #{ "type": "integer" },
        "remaining": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "details": #{ "type": "array", "maxItems": 80, "items": #{
            "type": "object",
            "required": ["file", "before", "after", "fix"],
            "properties": #{
                "file": #{ "type": "string" },
                "before": #{ "type": "string" },
                "after": #{ "type": "string" },
                "fix": #{ "type": "string" },
            }
        } },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["gates", "failed_test_files", "summary"],
    "properties": #{
        "gates": #{
            "type": "array", "maxItems": 6,
            "items": #{
                "type": "object",
                "required": ["name", "pass", "detail"],
                "properties": #{
                    "name": #{ "type": "string" },
                    "pass": #{ "type": "boolean" },
                    "detail": #{ "type": "string" },
                },
            },
        },
        "failed_test_files": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

let fix_gate_schema = #{
    "type": "object",
    "required": ["fixed", "summary", "remaining_errors"],
    "properties": #{
        "fixed": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "remaining_errors": #{ "type": "string" },
    },
};

const shared_rules = `
项目：erp-client (Next.js 16 App Router 纯 SPA, React 19, TanStack Query v5, TanStack Form, shadcn/ui)
根目录：/Users/huangjiajiang/Development/erp  应用目录：erp-client
必读：先阅读 erp-client/AGENTS.md 了解架构与目录约定；扫描时只改 erp-client 下用户可见文案，不改后端与配置。
`;

const copy_rules = `
# 文案审计与改写标准（技术化 -> 业务化）

## 受众
业务操作人员：采购、销售、财务、仓库、运营、审计员。非研发。对技术实现无认知，按业务流程与单据操作。

## 判定为“偏技术化、不利于理解”的条件（任一即改）
- A) 暴露实现细节：文案中出现 API / DTO / Entity / Schema / Payload / Params / Query / Mutation / Hook / Component / Props / State / Store / Cache / Retry / Polling / Webhook / Cron / Job / Queue / Index / Migration / Transaction / Idempotency / Endpoint / Request / Response / JSON / YAML / SDK / Token / Header / Body 等研发术语，且直接面向用户（toast/dialog/empty/alert/placeholder/button/label/表单校验提示）
- B) 暴露 HTTP/系统错误码：直接展示 400/401/403/404/500、Validation failed、Network Error、Fetch failed、Timeout、Exception、Stack trace、Null/Undefined/NaN、Failed to fetch 等未翻译的系统错误
- C) 英文技术动词未业务化：如 Submit / Post / Persist / Fetch / Load / Sync / Polling / Revalidate 等在中文语境下未转为 保存/提交/加载/同步/刷新 等
- D) 缺少可操作指引：仅说“失败/错误”不说原因与下一步（如 “操作失败” -> 应为 “保存失败，请检查必填项后重试”；“网络错误” -> “网络连接异常，请稍后重试”）
- E) 术语不一致：同一业务概念多处文案不一致（如 审批/审核/Approve 混用；供应商/供货商/商家 混用；履约/履行/执行 混用），需统一为项目既有业务术语
- F) 语气生硬/机器化：如 “Invalid input” “Required field” “Error: ... ” 等未作本地化与口吻软化
- G) 占位符/空状态技术化：如 “No data: query returned empty array” -> “暂无数据”；“Loading...” -> “加载中” 并保持与现有文案风格一致

## 改写原则（按优先级）
- 1) 业务化：用业务动作与对象表述。例：
  - "API request failed" -> "请求失败，请稍后重试" / 更具体的 "保存失败，请检查网络后重试"
  - "Validation failed: name is required" -> "请填写名称"
  - "Fetch failed" / "Network Error" -> "网络连接异常，请稍后重试"
  - "500 Internal Server Error" -> "服务暂时不可用，请稍后重试"
  - "Idempotency key missing" -> "提交已处理，请勿重复操作，刷新后查看结果"
  - "Sync polling..." -> "数据同步中，请稍候"
  - "Transaction failed" -> "操作未完成，请重试"
- 2) 可操作：说明原因 + 下一步。避免只说失败，要告诉用户怎么修（检查必填、检查网络、联系管理员、刷新重试等）
- 3) 一致性：沿用项目已有业务词典（采购单/销售单/合同/审批/履约/对账/结算/发票/收款/付款/库存/仓库/商品/物料/客户/供应商）。不新造词，不中英混用。参考 erp-client/lib/ui-text.ts 与各 feature 的 labels.ts / presentation.ts 已有表述
- 4) 简洁友好：中文语境用中文标点与口吻；英文语境保持简洁动词短语但去技术化（如 "Failed to load" -> "无法加载，请重试"）
- 5) 保持行为：仅改用户可见字符串（toast title/description、Dialog title/description、Alert、Button、Empty、Placeholder、表单校验 message、confirm 文案等），不改逻辑、校验规则、queryKey、路由、权限与数据字段名；代码中的变量名/注释/日志（console）不改
- 6) 最小改动：每个文件一次性改完；若文案来自后端错误透传（lib/api/errors.ts 等），在前端展示层做业务化映射而非改接口契约
- 7) 可验证：改前改后各记录 excerpt；grep 可复核

## 扫描方式（必须执行）
- 用 grep 搜用户可见文案：toast|Toast|alert|Alert|dialog|Dialog|empty|Empty|placeholder|Placeholder|Failed|Error|Exception|Validation|Network|Fetch|API|DTO|Payload|Schema|Transaction|Idempotency|Sync|Polling|Webhook|Retry|Loading
- 辅以搜中文技术化：接口|请求失败|异常|超时|校验失败 等
- 逐文件阅读：确认字符串是否最终渲染给用户（tsx 中 JSX 文本、toast() 参数、Alert/Dialog/Empty 组件 props、zod message、form 校验提示）；代码注释与类型定义不算
- 重点目录：components/business, components/ui/empty.tsx 等空/反馈组件, features/**/components, features/**/pages, features/**/lib/presentation.ts, features/**/lib/labels.ts, app/(workspace)/**, lib/ui-text.ts

## 输出与计数
- files_scanned：实际阅读的 .tsx/.ts 文件数
- issues_found/issues_fixed：发现并改写的技术化文案条数
- files_modified：实际落盘文件（repo 相对路径）
- remaining：无法自动改写的（如需产品确认的术语统一）列出并说明原因
- details：每条 before/after/fix 一句话，需包含文件与原文本 excerpt
`;

const verify_prompt = `你是 erp-client 文案改写的验证门禁。根目录 /Users/huangjiajiang/Development/erp/erp-client。

按顺序执行门禁（每步截取前 80 行输出）：
1. lint: npm run lint 或 npx oxlint .  (以 package.json 中实际脚本为准，优先 npm run lint)
2. typecheck: npx tsc --noEmit --incremental false
3. tests: npx vitest run  (若无 vitest 则跳过，记为 pass=true detail="no vitest")
4. build: 仅当 1-3 全部通过时执行 npm run build，否则记为 pass=false detail="skipped because earlier gate failed"

对 failing tests 收集失败文件路径（erp-client/... 形式，最多 60）。不要自行修复，仅报告。

输出 gates（每项 name/pass/detail 对应 lint/typecheck/tests/build）、failed_test_files、summary 一句话。`;

let shards = [
    #{ label: "shard-1-shell-lib", dirs: "erp-client/components/business, erp-client/components/ui, erp-client/lib, erp-client/app", desc: "壳层与通用组件、文案工具与空/反馈组件" },
    #{ label: "shard-2-master-data", dirs: "erp-client/features/master-data", desc: "主数据（物料/分类/品牌/仓库等，文案量大）" },
    #{ label: "shard-3-sales-customers", dirs: "erp-client/features/sales-orders, erp-client/features/customers, erp-client/features/contracts", desc: "销售单、客户、合同" },
    #{ label: "shard-4-purchase-supplier", dirs: "erp-client/features/purchase-orders, erp-client/features/supplier-orders, erp-client/features/supplier-offerings, erp-client/features/supplier-api-connections", desc: "采购与供应商域" },
    #{ label: "shard-5-payables-settlements", dirs: "erp-client/features/supplier-payables, erp-client/features/supplier-settlements, erp-client/features/finance-responsibilities, erp-client/features/procurement-responsibilities", desc: "应付/结算/财务责任" },
    #{ label: "shard-6-fulfillment-inventory", dirs: "erp-client/features/fulfillment-operations, erp-client/features/inventory, erp-client/features/mall-sync, erp-client/features/mall-consumption-orders", desc: "履约、库存、商城同步" },
    #{ label: "shard-7-receivables-quality", dirs: "erp-client/features/customer-receivables, erp-client/features/customer-quality, erp-client/features/card-funds-review, erp-client/features/card-business-analytics", desc: "应收、客诉、卡资金" },
    #{ label: "shard-8-approvals-workspace", dirs: "erp-client/features/approval-processes, erp-client/features/approval-workflow, erp-client/features/workspace, erp-client/features/work-items, erp-client/features/access-audit", desc: "审批、待办、工作台、审计" },
    #{ label: "shard-9-ops-analytics", dirs: "erp-client/features/execution-projections, erp-client/features/actual-profit-loss, erp-client/features/history-backfill, erp-client/features/import-opening, erp-client/features/integration-errors", desc: "执行预测、实盈、历史补录、导入、集成错误" },
    #{ label: "shard-10-entity-selectors", dirs: "erp-client/features/entity-selectors, erp-client/features/file-assets, erp-client/features/product-publications", desc: "选择器、附件、商品发布等" },
    #{ label: "shard-11-admin-auth", dirs: "erp-client/features/admin, erp-client/features/auth", desc: "管理入口与登录鉴权" },
];

phase("ScanRewrite");

let job_specs = [];
for s in shards {
    let p = shared_rules + "\n" + copy_rules + "\n";
    p += "你的分片：" + s.label + "\n";
    p += "负责目录（仅可改这些目录下的文件，逗号分隔）： " + s.dirs + "\n";
    p += "分片描述：" + s.desc + "\n";
    p += `
步骤（必须按序，面向用户文案）：

1) 枚举：用 list_dir / grep 列出分片内所有 .tsx/.ts 文件（重点 .tsx、lib/*.ts、components/*.tsx、pages/*.tsx）
2) 扫描：逐文件 grep 用户可见文案关键词（toast/Toast/alert/Alert/dialog/Dialog/empty/Empty/placeholder/Failed/Error/Exception/Validation/Network/Fetch/API/DTO/Schema/Payload/Transaction/Idempotency/Sync/Polling/Webhook/Retry/Loading 等），并阅读文件确认每处字符串是否最终渲染给用户（JSX 文本、toast 参数、Alert/Dialog/Empty props、表单校验 message、confirm 文案等）。注释与类型不算。
3) 判定：按上面的 A-G 判定技术化；对已业务化的文案不改，计入已达标
4) 改写：按改写原则 1-7 批量改写为业务语言，保持最小改动与一致性；同一文件多处一次性改完；若为手写英文技术错误透传，改展示层映射
5) 自检：对改动文件跑 npx oxlint <files> 与 npx tsc --noEmit --incremental false（慢则抽样），有报错就地修复；不得改 package.json、不得引入新依赖
6) 输出：严格按 fix_schema 输出 JSON；files_scanned 为扫描文件数，files_modified 为 repo 相对路径，issues_found/issues_fixed 为计数，remaining 为需人工确认的点，details 为每处 before/after/fix

约束：
- 仅可编辑分片 dirs 内的文件；不得改 .grok/workflows、backend、package.json、配置
- 不改行为：校验、默认值、queryKey、路由、权限均保持不变；仅改用户可见字符串
- 已业务化的文案不得重复改动
- 若分片内确无技术化文案，issues_found=0 且 files_modified=[]，并在 summary 说明已全量达标及证据（扫描文件数与抽样 grep 结果）

开始。
`;
    job_specs.push(#{ prompt: p, label: s.label, capability_mode: "all", output_schema: fix_schema });
}

let results = parallel(job_specs);

let retry_specs = [];
let retry_idx = [];
let i = 0;
for r in results {
    if r == () || r.success == false {
        let s = shards[i];
        let p = shared_rules + "\n" + copy_rules + "\n";
        p += "你的分片（重试）：" + s.label + "  dirs=" + s.dirs + "\n";
        p += "这是重试任务，按同样的扫描与改写步骤执行，输出 fix_schema。\n";
        retry_specs.push(#{ prompt: p, label: s.label + "-retry", capability_mode: "all", output_schema: fix_schema });
        retry_idx.push(i);
    }
    i += 1;
}
if retry_specs.len() > 0 {
    log("Retrying " + retry_specs.len().to_string() + " failed shard jobs");
    let retry_results = parallel(retry_specs);
    let k = 0;
    for idx in retry_idx {
        if retry_results[k] != () && retry_results[k].success == true {
            results[idx] = retry_results[k];
        }
        k += 1;
    }
}

phase("Verify");
let v1 = agent(verify_prompt, #{ label: "verify:round1", capability_mode: "execute", output_schema: verify_schema });

let rounds = 0;
while rounds < 2 {
    let all_pass = false;
    let failed = [];
    let terr = "";
    let lerr = "";
    if v1 != () && v1.success == true && v1.output.gates != () {
        all_pass = true;
        for g in v1.output.gates {
            if g.pass != true {
                all_pass = false;
                if g.name == "typecheck" && g.detail != () { terr = g.detail; }
                if g.name == "lint" && g.detail != () { lerr = g.detail; }
            }
        }
        if v1.output.failed_test_files != () {
            for f in v1.output.failed_test_files { failed.push(f); }
        }
    }
    if all_pass == true {
        rounds = 2;
    } else {
        phase("Fix");
        let fix_jobs = [];
        if terr != "" || lerr != "" {
            let excerpts = "";
            if terr != "" { excerpts += "typecheck:\n" + terr.sub_string(0, 2000) + "\n\n"; }
            if lerr != "" { excerpts += "lint:\n" + lerr.sub_string(0, 2000) + "\n"; }
            let gp = "你是门禁修复代理。根目录 /Users/huangjiajiang/Development/erp。只改 erp-client 下因此次文案改写引入的 lint/typecheck 错误；存量无关错误不要动，记入 remaining_errors。\n";
            gp += "错误摘要：\n" + excerpts + "\n";
            gp += "步骤：读相关文件，修复报错，跑 npx tsc --noEmit --incremental false 与 npx oxlint 自检。不要改 package.json、不要动 backend。输出 fixed/summary/remaining_errors。";
            fix_jobs.push(#{ prompt: gp, label: "fix:gates:" + rounds.to_string(), capability_mode: "all", output_schema: fix_gate_schema });
        }
        if failed.len() > 0 {
            let cap = fix_jobs.len();
            for f in failed {
                if cap >= 8 { break; }
                let fp = "你是测试修复代理。根目录 /Users/huangjiajiang/Development/erp。失败测试文件：" + f + "。仅改该文件所属特性目录下的实现/测试，修复因此次文案改动引入的回归；若测试对文案的断言需要更新则更新测试以匹配业务化新文案。跑 npx vitest run " + f + " 直到通过，并对改动文件跑 oxlint。不要改其他特性。输出 fixed/summary/remaining_errors。";
                fix_jobs.push(#{ prompt: fp, label: "fix:test:" + f, capability_mode: "all", output_schema: fix_gate_schema });
                cap += 1;
            }
        }
        if fix_jobs.len() > 0 {
            let fix_results = parallel(fix_jobs);
            let done = 0;
            for fr in fix_results { if fr != () && fr.success == true && fr.output.fixed == true { done += 1; } }
            log("Fix round " + rounds.to_string() + ": " + done.to_string() + "/" + fix_jobs.len().to_string() + " fixed");
        }
        phase("Verify");
        let opts = #{ label: "verify:round" + (rounds + 2).to_string(), capability_mode: "execute", output_schema: verify_schema };
        if v1 != () && v1.agent_id != () { opts.resume_from = v1.agent_id; }
        v1 = agent(verify_prompt, opts);
        rounds += 1;
    }
}

let final_pass = false;
let gates_text = "no gate data";
if v1 != () && v1.success == true && v1.output.gates != () {
    final_pass = true;
    gates_text = "";
    for g in v1.output.gates {
        let st = if g.pass == true { "PASS" } else { "FAIL" };
        gates_text += g.name + ": " + st + " — " + g.detail.sub_string(0, 600) + "\n";
        if g.pass != true { final_pass = false; }
    }
}

let total_scanned = 0;
let total_found = 0;
let total_fixed = 0;
let total_modified = 0;
let per_shard = "";
let all_details = [];
i = 0;
for r in results {
    let s = shards[i];
    if r == () || r.success == false {
        per_shard += "- " + s.label + ": AGENT FAILED\n";
    } else {
        let sc = 0; let fd = 0; let fx = 0; let mod = 0;
        if r.output.files_scanned != () { sc = r.output.files_scanned; total_scanned += sc; }
        if r.output.issues_found != () { fd = r.output.issues_found; total_found += fd; }
        if r.output.issues_fixed != () { fx = r.output.issues_fixed; total_fixed += fx; }
        if r.output.files_modified != () { mod = r.output.files_modified.len(); total_modified += mod; }
        per_shard += "- " + s.label + ": scanned " + sc.to_string() + ", found " + fd.to_string() + ", fixed " + fx.to_string() + ", modified " + mod.to_string();
        if r.output.summary != () { per_shard += " — " + r.output.summary; }
        per_shard += "\n";
        if r.output.details != () {
            for d in r.output.details { all_details.push(d); }
        }
    }
    i += 1;
}

let rep = "# 用户文案业务化改写报告\n\n";
rep += "Workflow: scan-ui-copy\n\n";
rep += "## 汇总\n\n";
rep += "扫描文件数：" + total_scanned.to_string() + "，发现技术化文案：" + total_found.to_string() + "，已改写：" + total_fixed.to_string() + "，改动文件数：" + total_modified.to_string() + "\n\n";
rep += "门禁 " + (if final_pass { "PASSED" } else { "FAILED" }) + "\n\n";
rep += "## 分片明细\n\n" + per_shard + "\n";
rep += "## 门禁详情\n\n" + gates_text + "\n";
if v1 != () && v1.success == true && v1.output.summary != () { rep += "Verify summary: " + v1.output.summary + "\n\n"; }
if all_details.len() > 0 {
    rep += "## 改写详情（抽样前 80）\n\n";
    let n = 0;
    for d in all_details {
        if n >= 80 { break; }
        rep += "- " + d.file + ": " + d.fix + " | before: " + d.before.sub_string(0, 160) + " -> after: " + d.after.sub_string(0, 160) + "\n";
        n += 1;
    }
    rep += "\n";
}
let remaining_text = "";
for r in results {
    if r != () && r.success == true && r.output.remaining != () {
        for rem in r.output.remaining { remaining_text += "- " + rem + "\n"; }
    }
}
if remaining_text != "" {
    rep += "## 仍需关注（需人工确认）\n\n" + remaining_text + "\n";
}
rep += "## 改写原则回顾\n\n";
rep += "技术术语（API/DTO/Schema/Payload/Transaction 等）已转为业务动作与可操作指引；错误提示已补充原因与下一步；术语已按业务词典统一。\n";
let path = write_scratch_file("ui-copy-report.md", rep);
let status = if final_pass { "passed" } else { "needs-attention" };
if total_found > total_fixed { status = "needs-attention"; }
complete(#{ status: status, report_path: path, report: rep, final_gates_passed: final_pass, total_scanned: total_scanned, total_found: total_found, total_fixed: total_fixed, total_modified: total_modified });
