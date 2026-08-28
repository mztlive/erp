let meta = #{
    name: "scan-required-stars",
    description: "扫描 erp-client 所有必填 input 缺少的红色星号并补上",
    when_to_use: "当需要审计全站表单必填项是否都有红色 * 标注并自动补齐时",
    phases: [
        #{ title: "ScanFix", detail: "并行扫描各特性域的必填字段，补齐红色星号" },
        #{ title: "Verify", detail: "lint / typecheck / tests 门禁" },
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
必读：先完整阅读 erp-client/AGENTS.md 与 components/form/*.tsx（TextField, TextareaField, SelectField, DateField, DateTimeField, PdfUploadField）
所有表单必须通过 components/form 的 useAppForm + form.AppField + field.*Field 绑定 TanStack Form；label 渲染走 components/ui/field.tsx 的 FieldLabel
`;

const audit_rules = `
# 审计与修复标准（必填 input 的红色 * 缺失）

什么是必填 input：
- 1) TanStack Form 字段的校验要求必填：zod schema 中 z.string().min(1) / z.string().nonempty / z.enum / z.object required 字段 / validators.onChange/onBlur/onSubmit 中标记为必填的字段
- 2) 业务必填：form.AppField 所在表单的提交校验会阻塞提交的字段；或 label 文案中已含 * / "必填" / required 语义的字段
- 3) 组件层必填：field.TextField / field.SelectField / field.DateField / field.TextareaField / field.PdfUploadField / OptionCombobox / TreeCombobox / selectors 等带有 required prop 的输入

判定为“缺失红色 *”的条件（任一即判定）：
- a) 基础组件缺实现：components/form/text-field.tsx, textarea-field.tsx, date-field.tsx, pdf-upload-field.tsx, select-field.tsx 的 FieldLabel 未根据 required 渲染红色星号。标准渲染为：{label}{required ? <span className="text-destructive">*</span> : null} 且 FieldLabel 内联 required 语义；SelectField 已有 required prop 但也缺星号，TextField 等则完全缺 required prop
- b) 调用点缺 required：某 field.*Field 的 label 对应 zod 必填字段（非 optional/nullable 且有 min(1)/nonempty/required 校验），但未传 required 导致星号不显示
- c) 手写星号：label="xxx*" 或 label="xxx *" 把星号写进文案而非用 required prop；应改为 label="xxx" required 且由组件渲染红色星号（text-destructive）
- d) 直接用 FieldLabel 的场景：features/**/components/*.tsx 中手写 <FieldLabel>标题</FieldLabel> / <FieldLabel htmlFor="...">标题</FieldLabel> 对应必填字段却无红色星号；需在 FieldLabel  children 后追加 <span className="text-destructive">*</span> 或改为带 required 语义的封装
- e) 业务选择器缺星号：components/business/option-combobox.tsx, tree-combobox.tsx, selectors.tsx, entity-comboboxes 等在 required 时未向 FieldLabel 传递星号
- f) 占位说明：若某字段仅在特定模式下必填（如编辑态可选、创建态必填），以创建/新增对话框的校验为准，仍需标注；若确为可选则不得加星

修复要求（按优先级）：
- A) 先修基础组件（最高优）：给 TextField/TextareaField/DateField/DateTimeField/PdfUploadField 补上 required?: boolean prop，并在 FieldLabel 中渲染红色星号。示例：
  TextField: type 中加 required?: boolean，解构中取 required，在 <FieldLabel> 内：{label}{required ? <span className="text-destructive">*</span> : null}
  同理 TextareaField/DateField/DateTimeField/PdfUploadField/SelectField（SelectField 已有 prop，只需补渲染）
  保持 hideLabel 逻辑：hideLabel 时星号也不显示（因 label sr-only）。星号颜色必须用 text-destructive（项目红色语义），不要用 text-red-500
- B) 调用点补 required：对每个必填字段的 field.*Field 加上 required prop，并清理 label 中的手写 "*"（如 label="名称*" -> label="名称" required）
  判断必填的依据：同文件内 useAppForm defaultValues + validators.onChange schema；若 schema 为 z.object({ name: z.string().min(1) }) 则 name 必填；若为 z.string().optional() / z.string().nullable().optional() 则可选
  若文件内无显式 schema，则以是否在提交前做 if (!value) 校验或服务端必填为准；保守策略：已含手写 "*" 的一律视为必填，需改为 required
- C) FieldLabel 直写场景：在 <FieldLabel>标题</FieldLabel> 后追加星号：<FieldLabel>标题<span className="text-destructive">*</span></FieldLabel>；若已有 htmlFor，保持不变
- D) 业务选择器：确保 required 透传到内部的 FieldLabel/Label；若其外层已用 Field 包裹，则在 FieldLabel 内补星
- E) 保持行为不变：不改校验、不改默认值、不改提交逻辑、不改 queryKey，仅改展示；label 文案除去多余星号与空格外不改
- F) 每个修复需可验证：grep 对应文件的 field.*Field required 与 label
- G) 若发现已正确实现（已有 required 且 FieldLabel 渲染星号），不得重复改动，计入已达标
- H) 修复后对改动文件跑 npx tsc --noEmit --incremental false 与 npx oxlint <files> 自检；不得改 package.json、不得引入新依赖
- I) 记录：对每个文件记录 missing_found / missing_fixed / 具体 fix 点（文件:行, before->after 一句话）

扫描方式：
- 从文件列表出发，用 grep 搜 field\\.TextField|field\\.SelectField|field\\.DateField|field\\.TextareaField|field\\.PdfUploadField|FieldLabel|required|label="[^"]*\\*"
- 逐文件阅读：确认每个 label 是否对应必填校验，是否已用 required 渲染星号
- 重点目录：components/form, components/business, components/ui/field.tsx, features/<feature>/components, features/<feature>/pages, app/(workspace)/**/page.tsx
`;

const verify_prompt = `你是 erp-client 必填星号修复的验证门禁。根目录 /Users/huangjiajiang/Development/erp/erp-client。

按顺序执行门禁（每步截取前 80 行输出）：
1. lint: npm run lint 或 npx oxlint .  (以 package.json 中实际脚本为准，优先 npm run lint)
2. typecheck: npx tsc --noEmit --incremental false
3. tests: npx vitest run  (若无 vitest 则跳过，记为 pass=true detail="no vitest")
4. build: 仅当 1-3 全部通过时执行 npm run build，否则记为 pass=false detail="skipped because earlier gate failed"

对 failing tests 收集失败文件路径（erp-client/... 形式，最多 60）。不要自行修复，仅报告。

输出 gates（每项 name/pass/detail 对应 lint/typecheck/tests/build）、failed_test_files、summary 一句话。`;

let shards = [
    #{ label: "shard-1-base-form", dirs: "erp-client/components/form, erp-client/components/ui/field.tsx, erp-client/components/ui/label.tsx", desc: "基础表单组件（TextField 等 6 个 + FieldLabel），最高优先修" },
    #{ label: "shard-2-business-selectors", dirs: "erp-client/components/business", desc: "业务选择器与共享组件（option-combobox, tree-combobox, selectors 等）" },
    #{ label: "shard-3-supplier-offerings", dirs: "erp-client/features/supplier-offerings", desc: "供应商供给（已知大量手写 * 需规范化）" },
    #{ label: "shard-4-supplier-payables-orders", dirs: "erp-client/features/supplier-payables, erp-client/features/supplier-orders, erp-client/features/supplier-settlements, erp-client/features/supplier-api-connections", desc: "供应商应付/订单/结算/连接" },
    #{ label: "shard-5-purchase-inventory", dirs: "erp-client/features/purchase-orders, erp-client/features/procurement-responsibilities, erp-client/features/inventory", desc: "采购单、履约责任、库存" },
    #{ label: "shard-6-sales-customers", dirs: "erp-client/features/sales-orders, erp-client/features/customers, erp-client/features/fulfillment-operations, erp-client/features/mall-sync, erp-client/features/mall-consumption-orders", desc: "销售/客户/履约/商城同步" },
    #{ label: "shard-7-contracts-finance", dirs: "erp-client/features/contracts, erp-client/features/finance-responsibilities, erp-client/features/customer-receivables, erp-client/features/customer-quality", desc: "合同、财务责任、应收与客诉" },
    #{ label: "shard-8-master-data-a", dirs: "erp-client/features/master-data/components/warehouse, erp-client/features/master-data/components/unit-of-measure, erp-client/features/master-data/components/brand, erp-client/features/master-data/components/category, erp-client/features/master-data/components/list", desc: "主数据组件 A（仓库/计量/品牌等）" },
    #{ label: "shard-9-master-data-b", dirs: "erp-client/features/master-data/components, erp-client/features/master-data/pages, erp-client/features/master-data/api", desc: "主数据其余组件/页面/API" },
    #{ label: "shard-10-ops-admin", dirs: "erp-client/features/admin, erp-client/features/auth, erp-client/features/approval-processes, erp-client/features/approval-workflow, erp-client/features/access-audit", desc: "后台账号/登录/审批/审计" },
    #{ label: "shard-11-remaining-features", dirs: "erp-client/features/actual-profit-loss, erp-client/features/card-business-analytics, erp-client/features/card-funds-review, erp-client/features/execution-projections, erp-client/features/history-backfill, erp-client/features/import-opening, erp-client/features/integration-errors, erp-client/features/product-publications, erp-client/features/workspace, erp-client/features/work-items, erp-client/features/entity-selectors, erp-client/features/file-assets", desc: "剩余特性域全量" },
    #{ label: "shard-12-app-shell", dirs: "erp-client/app, erp-client/lib, erp-client/hooks", desc: "路由与 lib/hooks 中直写 FieldLabel 的场景" },
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

1) 枚举：用 list_dir / grep 列出分片内所有 .tsx/.ts 文件，重点是 components/*.tsx, pages/*.tsx, dialogs/*.tsx
2) 扫描：对每个候选文件 grep field\\.TextField|field\\.SelectField|field\\.DateField|field\\.TextareaField|field\\.PdfUploadField|FieldLabel|required|label="[^"]*\\*"，阅读文件确认每个必填字段是否已有红色星号
3) 判定：按上面的 a-f 判定缺失；对已正确绑定的文件也要计数（计入 files_scanned，不计入 missing）
4) 修复：
   - 若分片含 components/form 或 components/ui/field.tsx：优先按 A 修复基础组件，确保 required 渲染 text-destructive 星号
   - 对调用点按 B/C/D 修复：补 required、去手写 *、补 FieldLabel 星号；保持最小改动；同一文件多处缺失一次性改完
5) 自检：对改动文件跑 npx oxlint <files> 与 npx tsc --noEmit --incremental false（可对单文件 tsc 快速检查，慢则抽样），有报错则就地修复
6) 输出：严格按 fix_schema 输出 JSON；files_scanned 为扫描文件数，files_modified 为 repo 相对路径，missing_found/missing_fixed 为计数，remaining 为仍无法自动修复的点，details 为每个文件的 before/after/fix 一句话

约束：
- 仅可编辑分片 dirs 内的文件；不得改 .grok/workflows、backend、package.json、配置
- 不得改行为：校验、默认值、queryKey、路由、权限均保持不变
- 已正确实现的必填星号不得改动
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
        let fix_jobs = [];
        if terr != "" || lerr != "" {
            let excerpts = "";
            if terr != "" { excerpts += "typecheck:\n" + terr.sub_string(0, 2000) + "\n\n"; }
            if lerr != "" { excerpts += "lint:\n" + lerr.sub_string(0, 2000) + "\n"; }
            let gp = "你是门禁修复代理。根目录 /Users/huangjiajiang/Development/erp。只改 erp-client 下因此次必填星号修复引入的 lint/typecheck 错误；明显与本次改动无关的存量错误不要动，记入 remaining_errors。\n";
            gp += "错误摘要：\n" + excerpts + "\n";
            gp += "步骤：读相关文件，修复报错，跑 npx tsc --noEmit --incremental false 与 npx oxlint 自检。不要改 package.json、不要动 backend。输出 fixed/summary/remaining_errors。";
            fix_jobs.push(#{ prompt: gp, label: "fix:gates:" + rounds.to_string(), capability_mode: "all", output_schema: fix_gate_schema });
        }
        if failed.len() > 0 {
            let cap = fix_jobs.len();
            for f in failed {
                if cap >= 8 { break; }
                let fp = "你是测试修复代理。根目录 /Users/huangjiajiang/Development/erp。失败测试文件：" + f + "。仅改该文件所属特性目录下的实现/测试，修复因此次星号改动引入的回归；若测试对 label 文案的断言需要更新则更新测试（去掉 * 后加 required 的情况）。跑 npx vitest run " + f + " 直到通过，并对改动文件跑 oxlint。不要改其他特性。输出 fixed/summary/remaining_errors。";
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

let rep = "# 必填星号修复报告\n\n";
rep += "Workflow: scan-required-stars\n\n";
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
let path = write_scratch_file("required-stars-report.md", rep);
let status = if final_pass { "passed" } else { "needs-attention" };
if total_found > total_fixed { status = "needs-attention"; }
complete(#{ status: status, report_path: path, report: rep, final_gates_passed: final_pass, total_scanned: total_scanned, total_found: total_found, total_fixed: total_fixed, total_modified: total_modified });
