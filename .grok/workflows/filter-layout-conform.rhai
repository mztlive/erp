let meta = #{
    name: "filter-layout-conform",
    description: "Fan out one agent per table page/module to conform every filter toolbar to docs/ui-filter-design.md using the 公司商品池 page as the reference benchmark, then verify with a full-suite run and an adversarial re-audit sample",
    when_to_use: "when every table page's filter layout and interaction must match the 公司商品池 benchmark exactly (single form, ListToolbar in BusinessTableFrame.toolbar, collapsible panel, chip row, 32px controls, URL as single source of truth)",
    phases: [
        #{ title: "Conform", detail: "one agent per page/module, parallel panel" },
        #{ title: "Verify", detail: "full test+lint+typecheck runner, adversarial re-audit sample" },
    ],
};

let PAGES = [
    #{
        label: "master-data",
        page: "erp-client/features/master-data/components/pages/products-list-page.tsx, suppliers-list-page.tsx, warehouses-list-page.tsx, brands-list-page.tsx, unit-of-measures-list-page.tsx, voucher-categories-list-page.tsx + category tree (category-tree-toolbar.tsx)",
        toolbar: "erp-client/features/master-data/components/list/product-list-toolbar.tsx, supplier-list-toolbar.tsx, dictionary-list-toolbar.tsx, category-tree-toolbar.tsx",
        doc: "docs/ui-workspaces/w14-basic-data.md",
        note: "6 list pages + 1 tree page share DictionaryListToolbar; the 4 master-data toolbars were already migrated per doc §3.1.1, verify each page independently",
    },
    #{
        label: "sellable-items-reference",
        page: "erp-client/features/master-data/components/pages/sellable-items-list-page.tsx",
        toolbar: "erp-client/features/master-data/components/list/sellable-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w14-basic-data.md",
        note: "REFERENCE page — audit-only, do NOT modify anything",
        audit_only: true,
    },
    #{
        label: "sales-orders",
        page: "erp-client/features/sales-orders/pages/sales-orders-list-page.tsx",
        toolbar: "erp-client/features/sales-orders/components/sales-orders-list-filter-bar.tsx",
        doc: "docs/ui-workspaces/w05-sales-orders.md",
    },
    #{
        label: "contracts",
        page: "erp-client/features/contracts/components/contracts-table-panel.tsx",
        toolbar: "inline ListToolbar inside contracts-table-panel.tsx",
        doc: "docs/ui-workspaces/w04-contracts.md",
    },
    #{
        label: "purchase-orders",
        page: "erp-client/features/purchase-orders/pages/purchase-orders-list-page.tsx",
        toolbar: "erp-client/features/purchase-orders/pages/purchase-orders-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w08-purchase-orders.md",
    },
    #{
        label: "supplier-orders",
        page: "erp-client/features/supplier-orders/pages/supplier-orders-list-page.tsx",
        toolbar: "erp-client/features/supplier-orders/components/supplier-orders-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w26-supplier-orders.md",
    },
    #{
        label: "supplier-settlements",
        page: "erp-client/features/supplier-settlements/components/settlement-list.tsx",
        toolbar: "erp-client/features/supplier-settlements/components/settlement-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w27-api-settlement.md",
    },
    #{
        label: "customers",
        page: "erp-client/features/customers/pages/customer-center-directory-table.tsx",
        toolbar: "erp-client/features/customers/pages/customer-center-directory-toolbar.tsx",
        doc: "docs/ui-workspaces/w03-customer-center.md",
    },
    #{
        label: "access-audit",
        page: "erp-client/features/access-audit/pages/access-audit-page.tsx",
        toolbar: "erp-client/features/access-audit/components/access-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w19-permissions-audit.md",
    },
    #{
        label: "product-publications",
        page: "erp-client/features/product-publications/pages/product-publications-list-page.tsx",
        toolbar: "erp-client/features/product-publications/components/publication-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w22-product-publication.md",
    },
    #{
        label: "supplier-offerings",
        page: "erp-client/features/supplier-offerings/components/supplier-offerings-toolbar.tsx",
        toolbar: "same file",
        doc: "docs/ui-workspaces/w21-supplier-offerings.md",
    },
    #{
        label: "execution-projections",
        page: "erp-client/features/execution-projections/components/execution-projection-list-panel.tsx",
        toolbar: "erp-client/features/execution-projections/components/execution-projection-filter-bar.tsx",
        doc: "docs/ui-workspaces/w23-execution-projection.md",
    },
    #{
        label: "customer-receivables",
        page: "erp-client/features/customer-receivables/pages/components/customer-receivables-table.tsx",
        toolbar: "erp-client/features/customer-receivables/pages/components/customer-receivables-toolbar.tsx",
        doc: "docs/ui-workspaces/w11-customer-receivables.md",
    },
    #{
        label: "supplier-payables",
        page: "erp-client/features/supplier-payables/pages/components/supplier-accounts-table.tsx",
        toolbar: "erp-client/features/supplier-payables/pages/components/supplier-accounts-toolbar.tsx",
        doc: "docs/ui-workspaces/w12-supplier-payables.md",
    },
    #{
        label: "inventory",
        page: "erp-client/features/inventory/pages/components/ledger-table.tsx",
        toolbar: "erp-client/features/inventory/pages/components/ledger-toolbar.tsx",
        doc: "docs/ui-workspaces/w10-inventory-ledger.md",
    },
    #{
        label: "mall-consumption-orders",
        page: "erp-client/features/mall-consumption-orders/components/list-page/orders-table.tsx",
        toolbar: "erp-client/features/mall-consumption-orders/components/list-page/filter-bar.tsx",
        doc: "docs/ui-workspaces/w25-mall-consumption-orders.md",
    },
    #{
        label: "mall-sync",
        page: "erp-client/features/mall-sync/pages/components/mall-sync-view-toolbar.tsx",
        toolbar: "same file",
        doc: "docs/ui-workspaces/w17-mall-sync-mapping.md",
    },
    #{
        label: "supplier-api-connections",
        page: "erp-client/features/supplier-api-connections/components/connection-list.tsx",
        toolbar: "erp-client/features/supplier-api-connections/components/connection-list-toolbar.tsx",
        doc: "docs/ui-workspaces/w20-supplier-api-connections.md",
    },
    #{
        label: "import-opening",
        page: "erp-client/features/import-opening/components/batch-list-view.tsx",
        toolbar: "inline within batch-list-view.tsx",
        doc: "docs/ui-workspaces/w18-import-opening.md",
    },
    #{
        label: "history-backfill",
        page: "erp-client/features/history-backfill/components/job-table.tsx",
        toolbar: "inline ListToolbar inside job-table.tsx",
        doc: "docs/ui-workspaces/w30-historical-consumption-backfill.md",
    },
    #{
        label: "fulfillment-operations",
        page: "erp-client/features/fulfillment-operations/components/queue/fulfillment-queue-toolbar.tsx",
        toolbar: "same file",
        doc: "docs/ui-workspaces/w07-procurement-confirmation-queue.md",
    },
    #{
        label: "actual-profit-loss",
        page: "erp-client/features/actual-profit-loss/components/profit-loss-rows-panel.tsx",
        toolbar: "inline ListToolbar inside profit-loss-rows-panel.tsx",
        doc: "docs/ui-workspaces/w16-actual-profit-loss.md",
    },
    #{
        label: "customer-quality",
        page: "erp-client/features/customer-quality/components/customer-quality-detail-table.tsx",
        toolbar: "erp-client/features/customer-quality/components/customer-quality-filter-card.tsx",
        doc: "docs/ui-workspaces/w15-customer-business-quality.md",
    },
    #{
        label: "card-business-analytics",
        page: "erp-client/features/card-business-analytics/components/card-business-drill-table.tsx",
        toolbar: "none found",
        doc: "docs/ui-workspaces/w28-card-consumption-analytics.md",
        note: "audit whether a filter area is required at all; if the table genuinely has no filters, report conforms with no changes",
    },
];

let conform_schema = #{
    "type": "object",
    "required": ["page", "conforms", "fixed", "blocked", "touched", "tests_run", "notes"],
    "properties": #{
        "page": #{ "type": "string" },
        "conforms": #{ "type": "boolean" },
        "fixed": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "blocked": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "touched": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "tests_run": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "notes": #{ "type": "string" },
    },
};

let suite_schema = #{
    "type": "object",
    "required": ["passed", "problems", "notes"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "problems": #{ "type": "array", "maxItems": 60, "items": #{ "type": "string" } },
        "notes": #{ "type": "string" },
    },
};

let sample_schema = #{
    "type": "object",
    "required": ["sampled", "notes"],
    "properties": #{
        "sampled": #{
            "type": "array",
            "maxItems": 12,
            "items": #{
                "type": "object",
                "required": ["page", "conforms", "violations"],
                "properties": #{
                    "page": #{ "type": "string" },
                    "conforms": #{ "type": "boolean" },
                    "violations": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
                },
            },
        },
        "notes": #{ "type": "string" },
    },
};

let target = PAGES;
if args != () {
    let only = args.only;
    if only != () && type_of(only) == "array" && only.len() > 0 {
        let filtered = [];
        for p in PAGES {
            if only.contains(p.label) { filtered.push(p); }
        }
        if filtered.len() > 0 { target = filtered; }
    }
}
log("conform targets: " + target.len().to_string() + " pages/modules");

let base = "你是一名 ERP 前端工程师，任务是把指定页面的筛选区域改造成与「公司商品池」完全一致的布局与交互，严格执行 docs/ui-filter-design.md。";
base += "先完整阅读 docs/ui-filter-design.md（重点 §1 统一结论、§3 筛选区视觉结构、§4 字段布局与组件选型、§5 状态模型、§8 公司商品池完整实现合同与 §8.2 通用状态模板、§9 响应式、§10 可访问性、§11 空错载、§12 反模式、§14 验收清单、§15 权威参考实现）。";
base += "再阅读基准实现（公司商品池，必须逐文件读）：erp-client/features/master-data/components/pages/sellable-items-list-page.tsx、erp-client/features/master-data/components/list/sellable-list-toolbar.tsx、erp-client/features/master-data/hooks/use-sellable-list-filters.ts、erp-client/features/master-data/hooks/use-sellable-list-state.ts、erp-client/features/master-data/lib/sellable-supply-preset.ts；";
base += "共享组件契约：erp-client/components/business/list.tsx（ListToolbar / BusinessTableFrame / surfacePanelClassName）、erp-client/components/business/fixed-option-radio-filter.tsx、fixed-option-checkbox-filter.tsx、option-combobox.tsx、filter-chip.tsx、erp-client/components/business/page.tsx。";
base += "用户可见文案遵守 docs/ui-glossary.md；业务只读/可写边界与禁止展示字段按对应工作面文档。";
base += "验收硬性规则（第 14 节验收清单 + 第 12 节反模式，逐条核对）：";
base += "（1）筛选区必须位于被过滤结果的同一主表面：表格页用 BusinessTableFrame.toolbar 承载整个筛选 form，禁止 ListToolbar 放在 frame 外或平行另一条工具栏；";
base += "（2）整页只允许一个语义 <form>：收起态提交箭头（搜索框尾部 icon-xs 按钮，aria-label「应用搜索与筛选」）与展开态面板底部唯一主按钮「应用全部筛选」调用同一个 apply 函数；展开后搜索框尾部箭头必须隐藏；";
base += "（3）结构化字段全部在可折叠面板内（更多筛选按钮 type=button，aria-expanded/aria-controls，面板 id 唯一），面板是筛选卡内部第二层，用 border-t pt-3 分隔，不嵌套 Card；底部必须有一行：范围说明「将同时应用上方关键词和以下筛选条件；结果也用于导出。」+「重置更多条件」+「应用全部筛选」；";
base += "（4）已生效条件全部以 FilterChip 显示在 ListToolbar.secondary 独立 chip 行（文案「已筛选」），末尾「清空全部」；来源锁定参数（customerId 等）必须显性 chip，禁止隐形查询参数；";
base += "（5）状态模型三层：Applied 在 URL（唯一事实源，router.replace scroll:false，默认值不写 URL，page 归 1）、Draft 在本地受控 state（不触发请求）、UI 态本地 state；query key / 导出 / 计数 / 摘要 / 空态只读 Applied；Draft 变化不得请求；非法枚举值解析时降级默认；";
base += "（6）「清空全部」同时重置 Draft、错误、面板、URL 筛选参数与分页，但不得删排序/视图/期间/导航上下文；「重置更多条件」只清结构化条件，保留 q 和快捷筛选，保持面板展开；";
base += "（7）筛选区所有可见控件统一 32px：Button 用默认档（不要 size=sm，该档是 28px），FixedOptionRadioFilter/CheckboxFilter 已内置 h-8，Input/Combobox 由 ListToolbar 子树收敛；只改筛选区控件，绝对不要动表格行内动作按钮的尺寸；";
base += "（8）可访问性：/ 聚焦搜索（忽略输入框/文本域/弹层），计数 aria-live=polite，错误 role=alert + aria-invalid/aria-describedby，面板 aria-label，不用颜色作唯一表达；";
base += "（9）空态：筛选无结果 BusinessEmptyState kind=filter 且动作用同一个 clearAllFilters，工具栏常驻不卸载；错误态用 BusinessFailureState + query.refetch，不得卸载筛选区；";
base += "（10）每个被查询消费的参数必须有可见控件或可移除 chip；有结构化条件的初始深链展开面板，提交成功后 URL 回填不得再次强制展开；面板展开态不回填重置。";
base += "实现边界：不得修改 erp-client/components/business/* 的现有行为；确需扩展时只能增加兼容的可选 prop，且必须在 notes 中说明；不得向 globals.css 增加页面私有选择器；不得改动表格行内按钮、表格列或业务数据逻辑。";
base += "完成后必须运行本模块相关单元测试（cd /Users/huangjiajiang/Development/erp/erp-client && npx vitest run <相关测试文件>，用通配符覆盖你改动的文件）与 npx oxlint <改动的文件>，保证通过；tests_run 记录实际执行的命令。";
base += "若页面已经完全符合规范，不要做任何修改，conforms=true，fixed=[]。";

phase("Conform");
let jobs = [];
for p in target {
    let audit_only = p.audit_only == true;
    let prompt = base + "本任务页面信息：" + json_encode(p) + "。";
    if audit_only {
        prompt += "这是基准参考页，只做审计：逐条核对后报告，禁止修改任何文件。";
    } else {
        prompt += "这是需要改造的页面。对照基准实现逐条核对后修复所有偏差；若某项偏差因业务或共享组件限制无法修复，写进 blocked（conforms 仍为 false）。";
    }
    prompt += "输出必须符合 output_schema：conforms 表示页面最终（或本来）符合规范；fixed 列出每条已修复偏差（一行一条，说明文件与改法）；blocked 列出无法修复的偏差；touched 列出修改过的文件；tests_run 列出实际运行的测试/lint 命令；notes 说明页面业务边界与任何需要人确认的点。";
    jobs.push(#{
        prompt: prompt,
        label: "conform:" + p.label,
        capability_mode: if audit_only { "read-only" } else { "read-write" },
        output_schema: conform_schema,
    });
}
let results = parallel(jobs);

let conformed = [];
let blocked_pages = [];
let failed = [];
let i = 0;
for r in results {
    let label = "page-" + i.to_string();
    if r == () {
        failed.push(label + " (agent failed, no output)");
        i += 1;
        continue;
    }
    if r.success && r.output != () && r.output.page != () {
        label = r.output.page;
        if r.output.conforms == true && r.output.blocked.len() == 0 {
            conformed.push(label);
        } else {
            blocked_pages.push(label + " -> " + r.output.notes);
        }
        if !r.success { failed.push(label); }
    } else {
        failed.push(label + " (failed or unusable output)");
    }
    i += 1;
}
log("conformed: " + conformed.len().to_string() + ", blocked/partial: " + blocked_pages.len().to_string() + ", failed: " + failed.len().to_string());

phase("Verify");
let suite_prompt = "在 /Users/huangjiajiang/Development/erp/erp-client 下执行完整验证，按顺序运行并记录每条命令的退出码与关键输出：";
suite_prompt += "（1）npm test（完整 vitest 套件）；";
suite_prompt += "（2）npx oxlint .；";
suite_prompt += "（3）npx tsc --noEmit。";
suite_prompt += "任何一条失败都视为整体未通过。problems 列出每条失败的具体输出摘录（文件、断言或错误行），最多 60 条；notes 记录运行时间与通过的套件数。不要修改任何文件。";
let verify_jobs = [
    #{
        prompt: suite_prompt,
        label: "verify:full-suite",
        capability_mode: "execute",
        output_schema: suite_schema,
    },
];
let sample_pages = [];
let j = 0;
for p in target {
    if j % 4 == 0 { sample_pages.push(p); }
    j += 1;
}
let sample_list = "";
for p in sample_pages {
    sample_list += p.label + " (" + p.page + "); ";
}
let sampler_prompt = "你是对抗性复核员。对下面这些页面逐一重新审计（只读，禁止修改文件），核对它们是否符合 docs/ui-filter-design.md 第 14 节验收清单与第 12 节反模式，并以「公司商品池」实现（erp-client/features/master-data/components/pages/sellable-items-list-page.tsx 与 sellable-list-toolbar.tsx）为基准：";
sampler_prompt += sample_list;
sampler_prompt += "对每个页面：先读它的页面组件、工具栏组件与状态 hook，再对照清单逐条找违规。conforms=true 只在你亲自读过文件且找不到任何违规时成立；发现任何违规必须写进 violations（含文件与具体问题）。不得把「未检查」当成「合规」。";
let verify_jobs_2 = [
    #{
        prompt: sampler_prompt,
        label: "verify:re-audit-sample",
        capability_mode: "read-only",
        output_schema: sample_schema,
    },
];
let suite_results = parallel(verify_jobs);
let sample_results = parallel(verify_jobs_2);

let suite_passed = false;
let suite_problems = [];
if suite_results[0] != () && suite_results[0].success && suite_results[0].output != () {
    suite_passed = suite_results[0].output.passed == true;
    if suite_results[0].output.problems != () {
        for pr in suite_results[0].output.problems { suite_problems.push(pr); }
    }
}
let sample_violations = [];
if sample_results[0] != () && sample_results[0].success && sample_results[0].output != () {
    if sample_results[0].output.sampled != () {
        for s in sample_results[0].output.sampled {
            if s.conforms != true {
                sample_violations.push(s.page + " -> " + json_encode(s.violations));
            }
        }
    }
}

let report = "## 筛选布局统一改造报告\n\n";
report += "- 目标页面/模块数：" + target.len().to_string() + "\n";
report += "- 已合规：" + conformed.len().to_string() + "\n";
report += "- 有未解决偏差：" + blocked_pages.len().to_string() + "\n";
report += "- agent 失败：" + failed.len().to_string() + "\n";
report += "- 全套件验证通过：" + (if suite_passed { "是" } else { "否" }) + "\n";
report += "- 抽样复核发现的违规页面数：" + sample_violations.len().to_string() + "\n\n";
report += "### 未合规 / 受阻页面\n";
for b in blocked_pages { report += "- " + b + "\n"; }
report += "### Agent 失败\n";
for f in failed { report += "- " + f + "\n"; }
report += "### 全套件问题\n";
for pr in suite_problems { report += "- " + pr + "\n"; }
report += "### 抽样复核违规\n";
for v in sample_violations { report += "- " + v + "\n"; }

let report_path = write_scratch_file("report.md", report);
complete(#{
    summary: "conformed=" + conformed.len().to_string()
        + ", blocked=" + blocked_pages.len().to_string()
        + ", failed=" + failed.len().to_string()
        + ", suite_passed=" + (if suite_passed { "yes" } else { "no" })
        + ", sample_violations=" + sample_violations.len().to_string(),
    conformed: conformed,
    blocked: blocked_pages,
    failed: failed,
    suite_passed: suite_passed,
    suite_problems: suite_problems,
    sample_violations: sample_violations,
    report_path: report_path,
});
