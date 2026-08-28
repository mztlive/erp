let meta = #{
    name: "submit-loading-fix",
    description: "扫描并补齐所有 submit 操作缺失的 loading 状态：表单、mutation 按钮、对话框确认、幂等提交",
    when_to_use: "当需要全量审计 erp-client 的提交链路并补齐缺失的加载/禁用态时",
    phases: [
        #{ title: "ScanFix", detail: "并行扫描各特性域的提交按钮与 mutation，补齐 loading/disabled" },
        #{ title: "Verify", detail: "lint / typecheck / tests / build 门禁" },
        #{ title: "Fix", detail: "针对门禁失败的定向修复" },
    ],
};

let fix_schema = #{
    "type": "object",
    "required": ["summary", "files_scanned", "files_modified", "missing_found", "missing_fixed", "remaining", "details"],
    "properties": #{
        "summary": #{ "type": "string" },
        "files_scanned": #{ "type": "integer" },
        "files_modified": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "missing_found": #{ "type": "integer" },
        "missing_fixed": #{ "type": "integer" },
        "remaining": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "details": #{ "type": "array", "maxItems": 60, "items": #{
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
必读：先完整阅读 erp-client/AGENTS.md（SPA 约束、Query 规则、Form 规则、术语约束）
所有服务端变更必须经 TanStack Query useMutation；表单提交走 TanStack Form onSubmit + mutate/mutateAsync
`;

const audit_rules = `
# 审计与修复标准（submit 操作的 loading 缺失）

什么是 submit 操作：
- 1) 表单提交：form.handleSubmit / onSubmit 触发的 useMutation / mutateAsync
- 2) 按钮提交：Button / AlertDialogAction / DialogFooter 按钮的 onClick 中调用 mutate / mutateAsync / submitMutation / createMutation 等
- 3) 向导/步骤确认、抽屉/对话框确认、批量操作、导出、重试等触发服务端写入的动作

判定为“缺失 loading”的条件（任一即判定）：
- a) 调用了 mutation.mutate / mutateAsync 但按钮未绑定 disabled={mutation.isPending} 或 pending/disabled 未覆盖提交中态
- b) 对话框确认按钮（AlertDialogAction）未在 isPending 时禁用取消按钮且未切换文案为"提交中…" / "处理中…" / 带 Spinner
- c) 表单提交按钮未使用 components/form/submit-button.tsx 的 SubmitButton，且自定义 Button type="submit" 未通过 form.Subscribe 绑定 canSubmit/isSubmitting
- d) 同一组件内多个 mutation 共享一个确认按钮但仅绑定其中一个 isPending（需合并：const pending = a.isPending || b.isPending）
- e) 幂等提交（带 idempotencyKey）的按钮在 pending 时仍可重复点击（未禁用）
- f) 导出/批量任务类按钮未在 isPending 时禁用并显示 pending 文案

修复要求：
- A) 优先复用已有 mutation 的 isPending：const pending = mutation.isPending 或 合并 pending
- B) 按钮：disabled={pending} 或 disabled={pending || otherDisabled}；文案在 pending 时切换为"提交中…" / "处理中…" / "删除中…" 等与原有风格一致；若项目已有 Spinner 组件则加 <Spinner className="mr-1 animate-spin"/> 或 lucide 的 Loader2
- C) 对话框：AlertDialogCancel disabled={pending}；AlertDialogAction disabled={pending} + pending 文案
- D) 表单：若已在 form.onSubmit 中调用 mutateAsync，按钮改用 SubmitButton 或保持 but 用 form.Subscribe 暴露 isSubmitting；禁止在 Form 内裸 fetch
- E) 批量/导出：沿用已有 job 状态，若无单独 mutation 则以 mutation.isPending 为准
- F) 保持行为不变：不改 queryKey、不改校验、不改文案（仅在 pending 时切换）、不改路由
- G) 每个修复需可验证：grep 对应文件的 mutateAsync 与按钮 disabled 的配对
- H) 若发现已正确实现（已绑定 isPending 且有 pending 文案），不得重复改动，计入已达标
- I) 修复后运行：npx tsc --noEmit --incremental false（或 npx oxlint <改动文件>）自检；不得改 package.json、不得引入新依赖
- J) 记录：对每个文件记录 missing_found / missing_fixed / 具体 fix 点（文件:行, before->after 一句话）

扫描方式：
- 从文件列表出发，用 grep 搜 useMutation / mutate( / mutateAsync / isPending / isSubmitting / SubmitButton / AlertDialogAction
- 逐文件阅读：确认每个 mutate 调用是否对应一个有 pending 绑定的按钮
- 重点目录：features/<feature>/components, features/<feature>/pages, features/<feature>/hooks, components/business, components/form, app/(workspace)/**/page.tsx 的透传组件
`;

const verify_prompt = `你是 erp-client 提交态修复的验证门禁。根目录 /Users/huangjiajiang/Development/erp/erp-client。

按顺序执行门禁（每步截取前 80 行输出）：
1. lint: npm run lint  或 npx oxlint .  (以 package.json 中实际脚本为准，优先 npm run lint)
2. typecheck: npx tsc --noEmit --incremental false
3. tests: npx vitest run  (若无 vitest 则跳过，记为 pass=true detail="no vitest")
4. build: 仅当 1-3 全部通过时执行 npm run build，否则记为 pass=false detail="skipped because earlier gate failed"

对 failing tests 收集失败文件路径（erp-client/... 形式，最多 60）。不要自行修复，仅报告。

输出 gates（每项 name/pass/detail 对应 lint/typecheck/tests/build）、failed_test_files、summary 一句话。`;

// 12 个分片，覆盖全量 features + 共享表单/业务组件
let shards = [
    #{ label: "shard-1-admin-auth-approval", dirs: "erp-client/features/admin, erp-client/features/auth, erp-client/features/approval-processes, erp-client/features/approval-workflow", desc: "后台账号/角色、登录、审批定义与审批流" },
    #{ label: "shard-2-supplier-core", dirs: "erp-client/features/supplier-payables, erp-client/features/supplier-settlements, erp-client/features/supplier-orders, erp-client/features/supplier-offerings, erp-client/features/supplier-api-connections", desc: "供应商应付/结算/订单/供给/连接" },
    #{ label: "shard-3-procurement-inventory", dirs: "erp-client/features/purchase-orders, erp-client/features/procurement-responsibilities, erp-client/features/inventory", desc: "采购单、履约责任、库存" },
    #{ label: "shard-4-sales-customers-fulfillment", dirs: "erp-client/features/sales-orders, erp-client/features/customers, erp-client/features/fulfillment-operations, erp-client/features/mall-sync, erp-client/features/mall-consumption-orders", desc: "销售/客户/履约/商城同步与核销" },
    #{ label: "shard-5-contracts-finance-receivables", dirs: "erp-client/features/contracts, erp-client/features/finance-responsibilities, erp-client/features/customer-receivables, erp-client/features/customer-quality", desc: "合同、财务责任、应收与客诉" },
    #{ label: "shard-6-master-data-a", dirs: "erp-client/features/master-data/components, erp-client/features/master-data/hooks, erp-client/features/master-data/lib", desc: "主数据组件/钩子/工具（大目录分半）" },
    #{ label: "shard-7-master-data-b", dirs: "erp-client/features/master-data/api, erp-client/features/master-data/pages", desc: "主数据 API 与页面" },
    #{ label: "shard-8-analytics-card-funds", dirs: "erp-client/features/actual-profit-loss, erp-client/features/card-business-analytics, erp-client/features/card-funds-review, erp-client/features/execution-projections", desc: "利润/卡业务分析/卡资金复核/执行预测" },
    #{ label: "shard-9-ops-backfill-sync", dirs: "erp-client/features/history-backfill, erp-client/features/import-opening, erp-client/features/integration-errors, erp-client/features/access-audit", desc: "历史回补、期初导入、集成异常、访问审计" },
    #{ label: "shard-10-product-workspace", dirs: "erp-client/features/product-publications, erp-client/features/workspace, erp-client/features/work-items, erp-client/features/entity-selectors", desc: "商品发布、工作台、待办、实体选择器" },
    #{ label: "shard-11-shared-ui-forms", dirs: "erp-client/components/form, erp-client/components/business, erp-client/components/ui, erp-client/app", desc: "共享表单/业务组件/UI 与路由透传" },
    #{ label: "shard-12-misc-remaining", dirs: "erp-client/features/file-assets, erp-client/lib, erp-client/hooks", desc: "文件资产与剩余 lib/hooks" },
];

phase("ScanFix");

let job_specs = [];
for s in shards {
    let p = shared_rules + "\n" + audit_rules + "\n";
    p += "你的分片：" + s.label + "\n";
    p += "负责目录（逗号分隔，仅可改这些目录下的文件）： " + s.dirs + "\n";
    p += "分片描述：" + s.desc + "\n";
    p += `
步骤（必须按序）：

1) 枚举：用 list_dir / grep 列出分片内所有 .tsx/.ts 文件，重点是 components/*.tsx, pages/*.tsx, hooks/*.ts, dialogs/*.tsx
2) 扫描：对每个候选文件 grep useMutation|mutate\\(|mutateAsync|AlertDialogAction|SubmitButton|isPending|isSubmitting，阅读文件确认每个 submit 调用是否已绑定 loading/disabled
3) 判定：按上面的 a-f 判定缺失；对已正确绑定的文件也要计数（计入 files_scanned，不计入 missing）
4) 修复：对每个缺失点按 A-J 修复；保持最小改动；同一文件多处缺失一次性改完；新增的 pending 变量就近定义在组件内，命名与已有风格一致（如 isPending / pending / submitting）
5) 自检：对改动文件跑 npx oxlint <files> 与 npx tsc --noEmit --incremental false（可对单文件 tsc 快速检查，慢则抽样），有报错则就地修复
6) 输出：严格按 fix_schema 输出 JSON；files_scanned 为扫描文件数，files_modified 为 repo 相对路径，missing_found/missing_fixed 为计数，remaining 为仍无法自动修复的点（如需后端配合），details 为每个文件的 before/after/fix 一句话

约束：
- 仅可编辑分片 dirs 内的文件；不得改 .grok/workflows、backend、package.json、配置
- 不得改行为：queryKey、校验、路由、权限、幂等键生成逻辑均保持不变
- 已正确实现的提交按钮不得改动
- 若分片内确实无缺失，missing_found=0 且 files_modified=[]，并在 summary 说明已全量达标及证据（扫描文件数与抽样 grep 结果）

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
        let p = shared_rules + "\n" + audit_rules + "\n";
        p += "你的分片（重试）：" + s.label + "  dirs=" + s.dirs + "\n";
        p += "这是重试任务，按同样的扫描与修复步骤执行，输出 fix_schema。\n";
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
        // 归类失败文件到分片，做定向修复
        let fix_jobs = [];
        // 通用门禁修复（lint/typecheck）
        if terr != "" || lerr != "" {
            let excerpts = "";
            if terr != "" { excerpts += "typecheck:\n" + terr.sub_string(0, 2000) + "\n\n"; }
            if lerr != "" { excerpts += "lint:\n" + lerr.sub_string(0, 2000) + "\n"; }
            let gp = "你是门禁修复代理。根目录 /Users/huangjiajiang/Development/erp。只改 erp-client 下因此次 submit-loading 修复引入的 lint/typecheck 错误；明显与本次改动无关的存量错误不要动，记入 remaining_errors。\n";
            gp += "错误摘要：\n" + excerpts + "\n";
            gp += "步骤：读相关文件，修复报错，跑 npx tsc --noEmit --incremental false 与 npx oxlint 自检。不要改 package.json、不要动 backend。输出 fixed/summary/remaining_errors。";
            fix_jobs.push(#{ prompt: gp, label: "fix:gates:" + rounds.to_string(), capability_mode: "all", output_schema: fix_gate_schema });
        }
        // 测试失败按文件分派（最多 8）
        if failed.len() > 0 {
            let cap = fix_jobs.len();
            for f in failed {
                if cap >= 8 { break; }
                let fp = "你是测试修复代理。根目录 /Users/huangjiajiang/Development/erp。失败测试文件：" + f + "。仅改该文件所属特性目录下的实现/测试，修复因此次 loading 改动引入的回归；若测试本身对 loading 的断言需要更新则更新测试，否则修实现。跑 npx vitest run " + f + " 直到通过，并对改动文件跑 oxlint。不要改其他特性。输出 fixed/summary/remaining_errors。";
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
        if r.output.missing_found != () { fd = r.output.missing_found; total_found += fd; }
        if r.output.missing_fixed != () { fx = r.output.missing_fixed; total_fixed += fx; }
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

let rep = "# Submit Loading 修复报告\n\n";
rep += "Workflow: submit-loading-fix\n\n";
rep += "## 汇总\n\n";
rep += "扫描文件数：" + total_scanned.to_string() + "，发现缺失：" + total_found.to_string() + "，已修复：" + total_fixed.to_string() + "，改动文件数：" + total_modified.to_string() + "\n\n";
rep += "门禁 " + (if final_pass { "PASSED" } else { "FAILED" }) + "\n\n";
rep += "## 分片明细\n\n" + per_shard + "\n";
rep += "## 门禁详情\n\n" + gates_text + "\n";
if v1 != () && v1.success == true && v1.output.summary != () { rep += "Verify summary: " + v1.output.summary + "\n\n"; }
if all_details.len() > 0 {
    rep += "## 修复详情（抽样前 60）\n\n";
    let n = 0;
    for d in all_details {
        if n >= 60 { break; }
        rep += "- " + d.file + ": " + d.fix + " | before: " + d.before.sub_string(0, 120) + " -> after: " + d.after.sub_string(0, 120) + "\n";
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
    rep += "## 仍需关注\n\n" + remaining_text + "\n";
}
let path = write_scratch_file("submit-loading-report.md", rep);
let status = if final_pass { "passed" } else { "needs-attention" };
if total_found > total_fixed { status = "needs-attention"; }
complete(#{ status: status, report_path: path, report: rep, final_gates_passed: final_pass, total_scanned: total_scanned, total_found: total_found, total_fixed: total_fixed, total_modified: total_modified });
