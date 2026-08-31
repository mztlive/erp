let meta = #{
    name: "add-actionable-element-ids",
    description: "扫描 erp-client 全部可操作元素，补充稳定、唯一、适合自动化脚本使用的 DOM id，并进行分片复审与门禁验证",
    when_to_use: "当需要为 ERP 前端按钮、链接、输入、选择器、菜单、标签页、表格行等交互目标补齐稳定 id 时",
    phases: [
        #{ title: "Foundation", detail: "完善共享组件的 id/idPrefix 透传与派生规则" },
        #{ title: "FeatureFix", detail: "按功能域并行扫描并补齐业务页面 id" },
        #{ title: "Audit", detail: "独立复审遗漏、重复与不稳定 id" },
        #{ title: "Repair", detail: "按复审结果定向修复" },
        #{ title: "Verify", detail: "lint、typecheck、tests、build 与静态覆盖门禁" },
    ],
};

let fix_schema = #{
    "type": "object",
    "required": ["shard", "files_scanned", "interactive_candidates", "ids_added", "ids_updated", "files_modified", "remaining", "summary"],
    "properties": #{
        "shard": #{ "type": "string" },
        "files_scanned": #{ "type": "integer" },
        "interactive_candidates": #{ "type": "integer" },
        "ids_added": #{ "type": "integer" },
        "ids_updated": #{ "type": "integer" },
        "files_modified": #{ "type": "array", "maxItems": 160, "items": #{ "type": "string" } },
        "remaining": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

let finding_schema = #{
    "type": "object",
    "required": ["file", "element", "issue", "suggested_id"],
    "properties": #{
        "file": #{ "type": "string" },
        "element": #{ "type": "string" },
        "issue": #{ "type": "string" },
        "suggested_id": #{ "type": "string" },
    },
};

let audit_schema = #{
    "type": "object",
    "required": ["shard", "files_scanned", "missing_count", "duplicate_risk_count", "unstable_count", "findings", "summary"],
    "properties": #{
        "shard": #{ "type": "string" },
        "files_scanned": #{ "type": "integer" },
        "missing_count": #{ "type": "integer" },
        "duplicate_risk_count": #{ "type": "integer" },
        "unstable_count": #{ "type": "integer" },
        "findings": #{ "type": "array", "maxItems": 120, "items": finding_schema },
        "summary": #{ "type": "string" },
    },
};

let repair_schema = #{
    "type": "object",
    "required": ["shard", "fixed_count", "files_modified", "remaining", "summary"],
    "properties": #{
        "shard": #{ "type": "string" },
        "fixed_count": #{ "type": "integer" },
        "files_modified": #{ "type": "array", "maxItems": 160, "items": #{ "type": "string" } },
        "remaining": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["gates", "static_audit", "browser_verification", "summary"],
    "properties": #{
        "gates": #{
            "type": "array", "maxItems": 8,
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
        "static_audit": #{ "type": "string" },
        "browser_verification": #{ "type": "string" },
        "summary": #{ "type": "string" },
    },
};

const root_rules = `
项目根目录：/Users/huangjiajiang/Development/erp
前端目录：erp-client
技术栈：Next.js 16 App Router 纯 SPA、React 19、Base UI、shadcn/ui、TanStack Form、TanStack Table。
开始前必须完整阅读 erp-client/AGENTS.md；只修改 erp-client 与本工作流目标直接相关的文件。backend/services/src/customer/profile.rs 与 backend/services/src/customer/profile/ 是用户已有未提交改动，严禁触碰、覆盖或格式化。

目标：给生产代码中每个真实可聚焦、可点击、可输入、可选择、可拖放或可键盘触发的 DOM 目标提供稳定且唯一的 id，供未来 Playwright、浏览器脚本和运维自动化直接使用。
`;

const id_rules = `
# id 规范

1. 使用实际 DOM id，不用 data-testid 代替；已有 data-testid 保留。
2. 静态 id 使用小写 kebab-case，格式优先为 feature-surface-purpose，例如 sales-orders-list-create、purchase-order-form-supplier、workspace-sidebar-logout。
3. 重复项必须带稳定业务键，例如 sales-orders-row-{row.id}-open。禁止仅用数组 index、随机数、时间戳或 React.useId 作为自动化 id。React.useId 只允许继续用于 aria 描述关系，不能作为新增的自动化选择器。
4. 动态片段若不保证只含 a-z、0-9、下划线或连字符，使用 @/lib/automation-id 的 toAutomationIdSegment(value) 清洗。共享基础阶段负责创建/完善该工具；不要各自复制清洗函数。
5. id 在同一浏览器文档中必须唯一。路由互斥可以复用概念，但同一页面同时挂载的表格、对话框、抽屉、列表、重复卡片和 portal 内容必须区分上下文或业务键。
6. id 必须落在真正接受 click/focus/type 的最终元素上。不要只给外层 div、Field、DialogContent 或表格容器加 id 来冒充内部按钮/输入已经覆盖。
7. render/asChild 模式要确认 id 最终透传到 Link、button 或触发器 DOM。共享组件若包含多个内部控件，使用 idPrefix/base id 派生 -trigger、-clear、-option-<value>、-close、-remove、-first-page 等子 id。
8. 修改输入 id 时同步 htmlFor、aria-describedby、错误和说明 id；不得破坏 label 关联。
9. 已有语义清楚、稳定、唯一的 id 保留；修复驼峰、过短、重复或上下文不清的 id 时同步所有引用和测试。
10. 不改变业务行为、文案、查询、表单校验、权限、路由、样式和网络请求；不引入依赖。
`;

const interactive_rules = `
# 必须覆盖的可操作元素

- 原生 button、a[href]、input（含 hidden file input）、textarea、select、contentEditable；
- Button、Link、SubmitButton、Input、Textarea、Checkbox、Switch、RadioGroupItem、SelectTrigger、TabsTrigger、Toggle、ToggleGroupItem；
- DropdownMenuItem/CheckboxItem/RadioItem、ContextMenuItem、MenubarItem、CommandItem、Combobox input/trigger/clear/item/chip remove；
- OptionCombobox、MultiOptionCombobox、TreeCombobox、业务实体选择器；每个选项、清除和树节点展开按钮也要能派生稳定 id；
- DatePicker/DateTimePicker/DateRangePicker 的触发、清除、日期按钮、时间输入和完成按钮；
- FileUpload 的 file input、选择、预览、移除和关闭；
- Dialog/Sheet/Drawer 的关闭、取消、确认；AlertDialogAction/Cancel；Popover/Dropdown 触发器；
- 数据表可交互行、行选择、多选全选、排序、列显隐、列移动/固定、分页；表格必须有调用方提供的稳定 id/idPrefix，行和内部控件由它派生；
- 轮播前后、分页链接、侧栏导航与折叠、面包屑链接、标签页、手风琴/折叠触发器；
- 任意带 onClick/onDoubleClick/onPointerDown/onMouseDown、键盘触发、tabIndex、role=button/link/menuitem/tab/option/checkbox/switch、拖放或上传行为的元素；
- 封装组件若接收 onCreate/onEdit/onDelete/onOpen/onRetry/onApply/onReset/onSubmit/onConfirm 等回调并在内部渲染交互控件，也必须暴露 id/idPrefix 并在调用点传入。

不计入：纯展示元素、没有交互处理器的 TableRow、aria-only 描述节点、测试文件中的演示组件、第三方库自动生成的 focus guard/sentinel。disabled 控件仍属于可操作界面，必须有 id。
`;

const scan_steps = `
# 扫描与修复步骤

1. 用 list_dir/grep 枚举负责目录内全部生产 .tsx/.jsx 文件；排除 node_modules、.next、*.test.*、*.spec.*、fixtures。
2. grep JSX 标签与 onClick/onKeyDown/tabIndex/role/contentEditable/onDrop/onFilesSelected 等，逐文件阅读，不能只靠简单正则计数。
3. 沿封装层追到最终 DOM：确认 id 是直接存在、由 props 透传，还是缺失。一个复合组件有多个内部操作时逐个覆盖。
4. 按 id 规范补齐；同文件所有遗漏一次修完。动态列表使用稳定实体键。共享组件优先新增可选 id/idPrefix prop 并派生内部 id，禁止在可复用组件内部写死会重复的静态 id。
5. 检查同页重复：同一组件多实例、多个 portal、列表循环、桌面/移动双渲染、条件分支是否会同时出现。
6. 对修改文件执行 npx oxfmt --write <files>，再执行 npx oxlint <files>。若改到共享类型，运行 npx tsc --noEmit --incremental false 并修复本次引入的问题。
7. 输出结构化报告。files_scanned 是实际阅读/扫描的生产 TSX 数，interactive_candidates 是确认过的交互目标数；remaining 只列确实无法安全自动决定的点。
`;

let foundation_prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules + "\n" + scan_steps;
foundation_prompt += `

你负责共享基础阶段，可编辑范围仅限：erp-client/components、erp-client/app、erp-client/lib/automation-id.ts、erp-client/AGENTS.md，以及为 automation-id 工具新增的定向测试文件。可以只读扫描全部 features 了解调用方式，但不得编辑 features。

额外要求：
- 创建或完善 @/lib/automation-id 的 toAutomationIdSegment(value: unknown): string；结果小写 kebab-safe，合并连续分隔符，去掉首尾分隔符，空结果回退为 item。写定向单元测试。
- 在 AGENTS.md 增加简短的“自动化 DOM id”规范，内容与本任务规则一致。
- TextField/TextareaField/SelectField/DateField/DateTimeField/PdfUploadField 等表单组件若只使用 field.name，增加可选 id 并用 resolvedId 同步 label/description/error；保留未传 id 时的兼容行为。
- SubmitButton、FileUpload、DatePicker 家族、Option/Multi/TreeCombobox、DataTable、通用反馈按钮、列表/页面动作、Dialog/Sheet/Drawer close、侧栏和其它复合组件逐一审计。内部多个控件必须由调用方 base id 派生。
- 对纯 UI primitive：如果 props 已完整透传 id，不要无意义改动；但 primitive 内部自行生成的额外按钮必须提供可派生 id 的 API。
- 全局单例壳（登录、工作区侧栏、账号菜单）可以直接使用清晰静态 id。
- 跨 feature 调用点缺 id 不在本阶段编辑，记录到 remaining，后续功能域代理会处理。

输出 fix_schema，shard 固定为 shared-foundation。
`;

let feature_shards = [
    #{ label: "master-data-components", dirs: "erp-client/features/master-data/components", prefix: "master-data" },
    #{ label: "master-data-pages", dirs: "erp-client/features/master-data/pages", prefix: "master-data" },
    #{ label: "sales-orders", dirs: "erp-client/features/sales-orders", prefix: "sales-orders" },
    #{ label: "supplier-payables-offerings", dirs: "erp-client/features/supplier-payables, erp-client/features/supplier-offerings", prefix: "supplier" },
    #{ label: "purchase-procurement", dirs: "erp-client/features/purchase-orders, erp-client/features/procurement-responsibilities", prefix: "procurement" },
    #{ label: "supplier-orders-settlements", dirs: "erp-client/features/supplier-orders, erp-client/features/supplier-settlements", prefix: "supplier" },
    #{ label: "fulfillment-inventory", dirs: "erp-client/features/fulfillment-operations, erp-client/features/inventory", prefix: "fulfillment-inventory" },
    #{ label: "customers-quality", dirs: "erp-client/features/customers, erp-client/features/customer-quality", prefix: "customers" },
    #{ label: "customer-receivables-finance", dirs: "erp-client/features/customer-receivables, erp-client/features/finance-responsibilities", prefix: "customer-finance" },
    #{ label: "supplier-api-selectors", dirs: "erp-client/features/supplier-api-connections, erp-client/features/entity-selectors, erp-client/features/party-selector", prefix: "supplier-api" },
    #{ label: "approval-admin-auth", dirs: "erp-client/features/approval-processes, erp-client/features/approval-workflow, erp-client/features/admin, erp-client/features/auth", prefix: "governance" },
    #{ label: "execution-profit", dirs: "erp-client/features/execution-projections, erp-client/features/actual-profit-loss", prefix: "execution" },
    #{ label: "mall", dirs: "erp-client/features/mall-consumption-orders, erp-client/features/mall-sync", prefix: "mall" },
    #{ label: "product-integration", dirs: "erp-client/features/product-publications, erp-client/features/integration-errors", prefix: "integration" },
    #{ label: "import-history-access", dirs: "erp-client/features/import-opening, erp-client/features/history-backfill, erp-client/features/access-audit", prefix: "operations" },
    #{ label: "card-contracts", dirs: "erp-client/features/card-business-analytics, erp-client/features/card-funds-review, erp-client/features/contracts", prefix: "card-contracts" },
    #{ label: "workspace-rest", dirs: "erp-client/features/workspace, erp-client/features/work-items, erp-client/features/file-assets", prefix: "workspace" },
];

let selected_shards = [];
let only = if args == () { () } else { args.only };
if only == () || only == "all" {
    selected_shards = feature_shards;
} else {
    for s in feature_shards {
        if s.label == only { selected_shards.push(s); }
    }
    if only == "shared-foundation" {
        selected_shards = [];
    } else if selected_shards.len() == 0 {
        pause("verification", "args.only 未匹配分片。可用：shared-foundation、all 或 feature_shards 中的 label。");
    }
}

phase("Foundation");
let foundation = ();
if only == () || only == "all" || only == "shared-foundation" {
    foundation = agent(foundation_prompt, #{ label: "foundation:shared", capability_mode: "all", output_schema: fix_schema });
    if foundation == () || foundation.success == false {
        pause("infra", "共享基础阶段失败；继续会让功能域无法可靠透传 id。请检查该代理结果后重跑。");
    }
}

if only == "shared-foundation" {
    let foundation_report = "# 可操作元素 id 工作流报告\n\n仅运行 shared-foundation。\n";
    if foundation != () && foundation.success == true {
        foundation_report += "扫描 " + foundation.output.files_scanned.to_string() + " 个文件，新增 " + foundation.output.ids_added.to_string() + " 个 id，更新 " + foundation.output.ids_updated.to_string() + " 个 id。\n\n";
        foundation_report += foundation.output.summary + "\n";
    }
    let foundation_path = write_scratch_file("actionable-id-report.md", foundation_report);
    complete(#{ status: "foundation-complete", report_path: foundation_path, report: foundation_report });
}

phase("FeatureFix");
let fix_jobs = [];
for s in selected_shards {
    let prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules + "\n" + scan_steps;
    prompt += "\n\n你的分片：" + s.label + "\n";
    prompt += "可编辑目录（仅限这些目录）：" + s.dirs + "\n";
    prompt += "默认 id 前缀：" + s.prefix + "（应继续细化到页面/对话框/列表/动作，不可只用这个前缀）\n";
    prompt += `
共享基础阶段已经先执行：优先使用共享组件新增的 id/idPrefix API 与 @/lib/automation-id。你可以阅读 components 和 lib，但不得编辑分片目录外文件；若共享 API 仍不足，先用当前可行方式覆盖调用点并在 remaining 精确记录需要共享修复的组件和 prop。

特别注意：
- form.AppField 内的 field.*Field、form.SubmitButton 也属于交互目标；为字段传入页面级 id，不能只因为底层默认 field.name 就跳过同页重复检查。
- column 定义中的行操作 Button/Link/Checkbox 必须使用 row 的稳定业务 id；列表卡片、桌面/移动双版本也必须区分。
- DataTable 调用必须传稳定 base id/idPrefix，让共享组件派生行、全选、分页、列设置 id。
- Option/Tree/MultiCombobox 与实体选择器必须传 base id；选项和内部清除/展开由共享层派生。
- Dialog、Sheet、Drawer、Popover、Dropdown、Tabs 的触发器和每个菜单项/标签页/确认/取消/关闭逐项覆盖。
- 不要把 data-testid 改成 id；二者可并存。

输出 fix_schema，shard 必须为 ` + s.label + `。
`;
    fix_jobs.push(#{ prompt: prompt, label: "fix:" + s.label, capability_mode: "all", output_schema: fix_schema });
}
let fix_results = parallel(fix_jobs);

let all_shards = [#{ label: "shared-foundation", dirs: "erp-client/components, erp-client/app, erp-client/lib/automation-id.ts", prefix: "shared" }];
for s in selected_shards { all_shards.push(s); }

const audit_steps = `
# 独立复审步骤（只读）

1. 重新枚举负责目录全部生产 TSX，不信任前一轮报告；grep 所有交互标签、事件处理器、tabIndex、role、拖放与复合组件。
2. 沿封装确认最终 DOM 的 id。对 props spread 只有在调用链明确传入 id 时才算覆盖。
3. 检查三个问题：缺 id；同一文档可能重复；使用 index/useId/随机数/时间戳/不安全动态片段导致不稳定。
4. 特别检查 menu/select/combobox item、clear/close、date calendar day、file input、DataTable row/selection/sort/pagination、移动/桌面双渲染。
5. findings 每项必须给出文件、具体元素或行附近代码、问题和建议 id。没有 finding 只能在确实扫描全部候选后返回空数组。
6. 严禁编辑文件。
`;

phase("Audit");
let audit_jobs = [];
for s in all_shards {
    let prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules + "\n" + audit_steps;
    prompt += "\n复审分片：" + s.label + "\n只读目录：" + s.dirs + "\n";
    prompt += "输出 audit_schema，shard=" + s.label + "。";
    audit_jobs.push(#{ prompt: prompt, label: "audit1:" + s.label, capability_mode: "read-only", output_schema: audit_schema });
}
let audits1 = parallel(audit_jobs);

phase("Repair");
let repair_jobs = [];
let repair_indices = [];
let ai = 0;
for a in audits1 {
    if a != () && a.success == true && a.output.findings != () && a.output.findings.len() > 0 {
        let s = all_shards[ai];
        let prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules;
        prompt += "\n\n你负责根据独立复审结果修复分片 " + s.label + "。仅可编辑：" + s.dirs + "。\n";
        prompt += "复审 findings（JSON）：\n" + json_encode(a.output.findings) + "\n\n";
        prompt += `逐项读实际代码验证；真实问题全部修复，误报不要硬改并在 remaining 说明。还要在相邻代码主动查同类遗漏。保持行为不变；动态 id 遵循稳定键和清洗规则。修改后执行 oxfmt、oxlint，必要时 tsc。输出 repair_schema。`;
        repair_jobs.push(#{ prompt: prompt, label: "repair:" + s.label, capability_mode: "all", output_schema: repair_schema });
        repair_indices.push(ai);
    }
    ai += 1;
}
let repair_results = [];
if repair_jobs.len() > 0 { repair_results = parallel(repair_jobs); }

phase("Audit");
let audit2_jobs = [];
for s in all_shards {
    let prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules + "\n" + audit_steps;
    prompt += "\n这是修复后的第二轮复审。分片：" + s.label + "\n只读目录：" + s.dirs + "\n";
    prompt += "必须重新检查实际代码，不能复用第一轮结论。输出 audit_schema。";
    audit2_jobs.push(#{ prompt: prompt, label: "audit2:" + s.label, capability_mode: "read-only", output_schema: audit_schema });
}
let audits2 = parallel(audit2_jobs);

let final_findings = [];
let failed_audits = [];
ai = 0;
for a in audits2 {
    let s = all_shards[ai];
    if a == () || a.success == false {
        failed_audits.push(s.label);
    } else if a.output.findings != () {
        for f in a.output.findings {
            final_findings.push(#{ shard: s.label, file: f.file, element: f.element, issue: f.issue, suggested_id: f.suggested_id });
        }
    }
    ai += 1;
}

let integration = ();
if final_findings.len() > 0 {
    phase("Repair");
    let integration_prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules;
    integration_prompt += `

你是最终跨分片修复代理。可编辑 erp-client 下与以下 findings 直接相关的生产代码；不得改 backend、依赖或业务行为。第二轮复审仍有问题，逐项验证并修复；可以同时调整共享组件 API 和功能调用点，解决跨目录透传。修改后运行 oxfmt、oxlint、tsc。若 finding 是误报，在 remaining 说明证据。findings：
`;
    integration_prompt += json_encode(final_findings);
    integration_prompt += "\n输出 repair_schema，shard=final-integration。";
    integration = agent(integration_prompt, #{ label: "repair:final-integration", capability_mode: "all", output_schema: repair_schema });
}

phase("Verify");
let verify_prompt = root_rules + "\n" + id_rules + "\n" + interactive_rules;
verify_prompt += `

你是最终验证门禁，只验证和报告，不修改文件。工作目录 /Users/huangjiajiang/Development/erp/erp-client。

按顺序执行：
1. npm run format:check
2. npm run lint
3. npx tsc --noEmit --incremental false
4. npm test
5. 仅当前四项都通过时 npm run build；否则标记 skipped because earlier gate failed。
6. 静态覆盖审计：用 Python 或 TypeScript AST 枚举 app/components/features 下生产 TSX 的原生交互元素、已知 UI/业务交互组件、事件处理器和 role/tabIndex。抽查至少 80 个跨功能文件，重点确认真正 DOM 上有稳定 id；统计仍明显缺失的文件和代表性位置。不要因为组件有 data-testid 就算通过。
7. 重复/不稳定审计：扫描静态 id 重复、模板 id 仅用 index、React.useId 被用于自动化目标、同组件桌面/移动双渲染重复。区分路由互斥与同页风险。
8. 浏览器验证：先检查是否存在可用 browser/Playwright 工具或仓库 E2E 基础设施。若可用，启动/复用前端并至少验证登录页、工作区侧栏、一个列表筛选/分页、一个表单/对话框，执行点击/输入并在 DOM 中确认 id 唯一；桌面与移动各一次。若确实无浏览器能力，不得伪造，browser_verification 明确写 unavailable 及已用的替代验证。

每项 gate 输出 name/pass/detail。静态审计若仍发现明确遗漏或重复，增加 gate name=actionable-id-audit pass=false。browser 不可用本身不令代码门禁失败，但必须如实报告。输出 verify_schema。
`;
let verification = agent(verify_prompt, #{ label: "verify:all", capability_mode: "execute", output_schema: verify_schema });

let final_pass = true;
let gates_text = "verification agent failed";
let browser_text = "unavailable";
let static_text = "no data";
if verification == () || verification.success == false {
    final_pass = false;
} else {
    gates_text = "";
    if verification.output.gates != () && type_of(verification.output.gates) == "array" {
        for g in verification.output.gates {
            let state = if g.pass == true { "PASS" } else { "FAIL" };
            gates_text += "- " + g.name + ": " + state + " — " + g.detail + "\n";
            if g.pass != true { final_pass = false; }
        }
    } else {
        final_pass = false;
        gates_text = "verification returned no iterable gates";
    }
    if verification.output.static_audit != () { static_text = verification.output.static_audit; }
    if verification.output.browser_verification != () { browser_text = verification.output.browser_verification; }
}
if failed_audits.len() > 0 { final_pass = false; }
if integration != () && integration.success == false { final_pass = false; }

let total_files = 0;
let total_candidates = 0;
let total_added = 0;
let total_updated = 0;
let total_modified = 0;
let shard_text = "";
if foundation != () && foundation.success == true {
    let sc = 0; let cand = 0; let added = 0; let updated = 0; let modified = 0; let summary = "no summary";
    if foundation.output.files_scanned != () { sc = foundation.output.files_scanned; }
    if foundation.output.interactive_candidates != () { cand = foundation.output.interactive_candidates; }
    if foundation.output.ids_added != () { added = foundation.output.ids_added; }
    if foundation.output.ids_updated != () { updated = foundation.output.ids_updated; }
    if foundation.output.files_modified != () && type_of(foundation.output.files_modified) == "array" { modified = foundation.output.files_modified.len(); }
    if foundation.output.summary != () { summary = foundation.output.summary; }
    total_files += sc;
    total_candidates += cand;
    total_added += added;
    total_updated += updated;
    total_modified += modified;
    shard_text += "- shared-foundation: scanned " + sc.to_string() + ", candidates " + cand.to_string() + ", added " + added.to_string() + ", updated " + updated.to_string() + ", files " + modified.to_string() + " — " + summary + "\n";
}
let fi = 0;
for r in fix_results {
    let s = selected_shards[fi];
    if r == () || r.success == false {
        shard_text += "- " + s.label + ": AGENT FAILED\n";
        final_pass = false;
    } else {
        let sc = 0; let cand = 0; let added = 0; let updated = 0; let modified = 0; let summary = "no summary";
        if r.output.files_scanned != () { sc = r.output.files_scanned; }
        if r.output.interactive_candidates != () { cand = r.output.interactive_candidates; }
        if r.output.ids_added != () { added = r.output.ids_added; }
        if r.output.ids_updated != () { updated = r.output.ids_updated; }
        if r.output.files_modified != () && type_of(r.output.files_modified) == "array" { modified = r.output.files_modified.len(); }
        if r.output.summary != () { summary = r.output.summary; }
        total_files += sc;
        total_candidates += cand;
        total_added += added;
        total_updated += updated;
        total_modified += modified;
        shard_text += "- " + s.label + ": scanned " + sc.to_string() + ", candidates " + cand.to_string() + ", added " + added.to_string() + ", updated " + updated.to_string() + ", files " + modified.to_string() + " — " + summary + "\n";
    }
    fi += 1;
}

let remaining_after_audit = final_findings.len();
let report = "# 可操作元素 DOM id 补齐报告\n\n";
report += "Workflow: add-actionable-element-ids\n\n";
report += "## 汇总\n\n";
report += "扫描文件（分片报告合计）：" + total_files.to_string() + "\n\n";
report += "确认交互候选：" + total_candidates.to_string() + "\n\n";
report += "新增 id：" + total_added.to_string() + "；更新既有 id：" + total_updated.to_string() + "；首轮改动文件：" + total_modified.to_string() + "\n\n";
report += "第二轮复审 findings：" + remaining_after_audit.to_string() + "；失败复审分片：" + failed_audits.len().to_string() + "\n\n";
report += "最终门禁：" + (if final_pass { "PASSED" } else { "NEEDS ATTENTION" }) + "\n\n";
report += "## 分片结果\n\n" + shard_text + "\n";
report += "## 门禁\n\n" + gates_text + "\n";
report += "## 静态审计\n\n" + static_text + "\n\n";
report += "## 浏览器验证\n\n" + browser_text + "\n\n";
if final_findings.len() > 0 {
    report += "## 第二轮复审遗留（最终整合代理已尝试修复）\n\n";
    let n = 0;
    for f in final_findings {
        if n >= 100 { break; }
        report += "- [" + f.shard + "] " + f.file + " — " + f.element + ": " + f.issue + "；建议 " + f.suggested_id + "\n";
        n += 1;
    }
    report += "\n";
}
if failed_audits.len() > 0 {
    report += "## 未完成复审分片\n\n";
    for s in failed_audits { report += "- " + s + "\n"; }
    report += "\n";
}
let report_path = write_scratch_file("actionable-id-report.md", report);
complete(#{
    status: if final_pass { "passed" } else { "needs-attention" },
    report_path: report_path,
    report: report,
    final_gates_passed: final_pass,
    total_files_scanned: total_files,
    total_interactive_candidates: total_candidates,
    total_ids_added: total_added,
    total_ids_updated: total_updated,
    initial_files_modified: total_modified,
    second_audit_findings: remaining_after_audit,
    failed_audits: failed_audits,
});
