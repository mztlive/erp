// 按波次实现组织数据范围合同剩余项，并以第 9 章静态门禁为放行条件。
// 运行：/workflow implement-org-data-scope
// Args:
//   wave: 可选。省略则先读合同状态，从最早未关闭阶段中第一个仍有可实施缺口的波次开始。
//         s2-consumers | s2-sales | s2-ui | s2-import | s2-adapters | s2-tasks
//         | s3-catalog | s3-funds | s3-workbench | s3-quality | s3-handover
//         | s4-master | s4-supply | s4-ops | inventory
//   batch_ids: 可选字符串数组。显式指定时不因合同“功能已接入”而跳过（仍按合同剩余缺口补，不重写）。
//   land: 可选布尔。true=改父工作区（多批次时顺序执行）；false=隔离 worktree 并行。
//         默认：单批次 true，多批次 false。
//   max_gate_rounds: 可选整数 1..16。第 9 章门禁「运行→整改」轮次上限，默认 8。
// 门禁：docs/organization-data-scope-gate.md；入口 bash backend/scripts/check-org-data-scope.sh
// 失败必须继续改代码并重跑，直到 STATIC_CHECKS_PASSED。不得白名单/忽略路径/把失败改警告。
// 选批：读 S2/S3 合同状态表，对照 catalog id。open/partial 才自动开做；
// blocked_acceptance（仅 A34/A36/真实库/业务验收）与 S2-11/第10章自动跳过。
// S2 未关闭（非本地检查通过/已验收）时不自动进入 S3/S4。

let meta = #{
    name: "implement-org-data-scope",
    description: "先读合同完成状态，从仍有可实施缺口的批次继续；S2 消费者/适配器/页面，S3 核心链路，S4 扩展。第 9 章静态门禁不通过则继续整改直到通过。不跑真实 MongoDB，不登记业务验收。",
    when_to_use: "继续完成 organization-data-scope-contract 的剩余实现。可省略 wave，默认按合同从最早未完成波次开始。实现后必须通过第 9 章静态门禁。",
    phases: [
        #{ title: "Select", detail: "读合同状态表，过滤 catalog 后选出仍须实现的批次" },
        #{ title: "Implement", detail: "按批次实现 DataScope v2 消费者、查询、页面与测试" },
        #{ title: "Review", detail: "对照合同与代码做对抗评审" },
        #{ title: "Repair", detail: "至多一轮修复未通过评审的批次" },
        #{ title: "Verify", detail: "定向单测与静态核对，不跑真实库" },
        #{ title: "Gate", detail: "执行第 9 章静态门禁；失败则整改并重跑直到通过" },
        #{ title: "Report", detail: "写入 scratch 报告并列出下一波" },
    ],
};

let implementation_schema = #{
    "type": "object",
    "required": ["batch_id", "completed", "files_changed", "commands", "summary", "blockers", "still_open"],
    "properties": #{
        "batch_id": #{ "type": "string", "minLength": 1 },
        "completed": #{ "type": "boolean" },
        "committed": #{ "type": "boolean" },
        "commit_sha": #{ "type": "string" },
        "branch": #{ "type": "string" },
        "files_changed": #{ "type": "array", "maxItems": 128, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 32, "items": #{ "type": "string" } },
        "summary": #{ "type": "string", "minLength": 1 },
        "blockers": #{ "type": "array", "maxItems": 32, "items": #{ "type": "string" } },
        "still_open": #{ "type": "array", "maxItems": 32, "items": #{ "type": "string" } },
    },
};

let review_schema = #{
    "type": "object",
    "required": ["batch_id", "approved", "findings", "evidence", "summary"],
    "properties": #{
        "batch_id": #{ "type": "string", "minLength": 1 },
        "approved": #{ "type": "boolean" },
        "findings": #{
            "type": "array",
            "maxItems": 24,
            "items": #{
                "type": "object",
                "required": ["severity", "file", "issue", "evidence", "remediation"],
                "properties": #{
                    "severity": #{ "type": "string" },
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                    "remediation": #{ "type": "string" },
                },
            },
        },
        "evidence": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "summary": #{ "type": "string", "minLength": 1 },
    },
};

let verification_schema = #{
    "type": "object",
    "required": ["batch_id", "all_pass", "commands", "evidence", "summary"],
    "properties": #{
        "batch_id": #{ "type": "string", "minLength": 1 },
        "all_pass": #{ "type": "boolean" },
        "commands": #{ "type": "array", "maxItems": 32, "items": #{ "type": "string" } },
        "evidence": #{ "type": "array", "maxItems": 32, "items": #{ "type": "string" } },
        "summary": #{ "type": "string", "minLength": 1 },
        "environment_blocked": #{ "type": "boolean" },
    },
};

let gate_schema = #{
    "type": "object",
    "required": ["passed", "exit_code", "status", "command", "findings", "summary"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "exit_code": #{ "type": "integer" },
        "status": #{ "type": "string", "minLength": 1 },
        "command": #{ "type": "string", "minLength": 1 },
        "files_scanned": #{ "type": "integer" },
        "findings": #{
            "type": "array",
            "maxItems": 96,
            "items": #{
                "type": "object",
                "required": ["rule", "path", "line", "message", "category"],
                "properties": #{
                    "rule": #{ "type": "string" },
                    "path": #{ "type": "string" },
                    "line": #{ "type": "integer" },
                    "message": #{ "type": "string" },
                    "category": #{ "type": "string" },
                },
            },
        },
        "not_proven": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string" } },
        "summary": #{ "type": "string", "minLength": 1 },
    },
};

let gate_fix_schema = #{
    "type": "object",
    "required": ["fixed", "files_changed", "summary", "remaining_findings"],
    "properties": #{
        "fixed": #{ "type": "boolean" },
        "files_changed": #{ "type": "array", "maxItems": 128, "items": #{ "type": "string" } },
        "summary": #{ "type": "string", "minLength": 1 },
        "remaining_findings": #{
            "type": "array",
            "maxItems": 96,
            "items": #{
                "type": "object",
                "required": ["rule", "path", "line", "message", "category"],
                "properties": #{
                    "rule": #{ "type": "string" },
                    "path": #{ "type": "string" },
                    "line": #{ "type": "integer" },
                    "message": #{ "type": "string" },
                    "category": #{ "type": "string" },
                },
            },
        },
        "exit_code": #{ "type": "integer" },
        "status": #{ "type": "string" },
        "blockers": #{ "type": "array", "maxItems": 32, "items": #{ "type": "string" } },
    },
};

let contract_status_schema = #{
    "type": "object",
    "required": ["s2_stage", "s3_stage", "s4_stage", "batches"],
    "properties": #{
        "s2_stage": #{ "type": "string", "minLength": 1 },
        "s3_stage": #{ "type": "string", "minLength": 1 },
        "s4_stage": #{ "type": "string", "minLength": 1 },
        "batches": #{
            "type": "array",
            "maxItems": 24,
            "items": #{
                "type": "object",
                "required": ["batch_id", "disposition", "contract_status", "remaining", "evidence"],
                "properties": #{
                    "batch_id": #{ "type": "string" },
                    "disposition": #{ "type": "string" },
                    "contract_status": #{ "type": "string" },
                    "remaining": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                },
            },
        },
    },
};

fn is_text(value) {
    type_of(value) == "string" && value != ""
}

fn is_map(value) {
    value != () && type_of(value) == "map"
}

fn is_arr(value) {
    type_of(value) == "array"
}

fn contains_text(items, wanted) {
    let found = false;
    if is_arr(items) {
        for item in items {
            if item == wanted {
                found = true;
            }
        }
    }
    found
}

fn is_gate_pass(r) {
    r != () && r.success && is_map(r.output) && r.output.passed == true && r.output.exit_code == 0 && r.output.status == "STATIC_CHECKS_PASSED"
}

fn is_gate_usable(r) {
    r != () && r.success && is_map(r.output) && is_text(r.output.status) && r.output.exit_code != () && r.output.passed != () && is_arr(r.output.findings)
}

fn gate_findings_json(r) {
    if is_gate_usable(r) {
        json_encode(r.output.findings)
    } else {
        "[]"
    }
}

fn gate_fingerprint(r) {
    if is_gate_usable(r) {
        fingerprint(json_encode(r.output.findings))
    } else if r != () {
        "unusable"
    } else {
        "missing"
    }
}

fn all_true(flags) {
    let ok = flags.len() > 0;
    for flag in flags {
        if flag != true {
            ok = false;
        }
    }
    ok
}

fn batch_ids_csv(selected) {
    let s = "";
    for batch in selected {
        if s != "" {
            s += ",";
        }
        s += batch.id;
    }
    s
}

fn is_allowed_disposition(value) {
    value == "open" || value == "partial" || value == "blocked_acceptance" || value == "out_of_scope" || value == "later_stage"
}

fn is_auto_eligible(row) {
    is_map(row) && (row.disposition == "open" || row.disposition == "partial")
}

fn stage_closed(value) {
    value == "本地检查通过" || value == "已验收"
}

fn stage_of_wave(wave) {
    if wave == "s3-catalog" || wave == "s3-funds" || wave == "s3-workbench" || wave == "s3-quality" || wave == "s3-handover" {
        "S3"
    } else if wave == "s4-master" || wave == "s4-supply" || wave == "s4-ops" {
        "S4"
    } else {
        "S2"
    }
}

fn status_field(row, name, fallback) {
    let out = fallback;
    if is_map(row) {
        if name == "disposition" && is_text(row.disposition) {
            out = row.disposition;
        } else if name == "contract_status" && is_text(row.contract_status) {
            out = row.contract_status;
        } else if name == "remaining" && is_text(row.remaining) {
            out = row.remaining;
        } else if name == "evidence" && is_text(row.evidence) {
            out = row.evidence;
        }
    }
    out
}

fn contract_status_prompt(ids_json) {
    let p = "";
    p += "读取组织数据范围合同的完成状态。必须用 read_file 打开下列文件，禁止凭记忆作答：\n";
    p += "1. docs/organization-data-scope-contract.md 第 11 章阶段表\n";
    p += "2. docs/organization-data-scope-s2.md 文首“状态”和第 4 章批次表\n";
    p += "3. docs/organization-data-scope-s3.md 文首“状态”和第 3 章批次表\n\n";
    p += "只为下列 catalog batch_id 各输出一行，不得增加或改写 id：\n";
    p += ids_json;
    p += "\n\n";
    p += "s2_stage、s3_stage、s4_stage 只能是：未开始、执行中、本地检查通过、已验收。从文档规范化，不要抄整句说明。\n";
    p += "disposition 只能是：open、partial、blocked_acceptance、out_of_scope、later_stage。\n";
    p += "分类规则：\n";
    p += "- open：未接入、未实施、未交付、范围消费未完成，且剩余工作是本 workflow 可写的生产代码与定向单测。\n";
    p += "- partial：部分接入，仍有本批 owns 内代码缺口（组织筛选、关联对象、变更单、旧读取器替换等）。只补缺口。\n";
    p += "- blocked_acceptance：剩余仅 A34/A36、第 4 节行为核验、真实 MongoDB、HTTP/浏览器或业务验收。S2-01 功能已接入且仅剩这些时必须用此项，不得标 open。\n";
    p += "- out_of_scope：S2-11、第 10 章上线准入、A 矩阵真实账号验收。本 workflow 不自动开做。\n";
    p += "- later_stage：S4 批次在 S2 或 S3 仍为执行中或未开始时。\n";
    p += "S3-03a/S3-03b/S3-03c 都对应 S3-03 行。S2-10 缺查询合同或代码为 partial；仅剩真实账号矩阵为 blocked_acceptance。\n";
    p += "contract_status 摘录表格“当前状态”。remaining 写仍须代码交付的缺口；blocked/out_of_scope 写清为何不能自动开做。evidence 引用文档与编号。\n";
    p += "必须读完三份文件再输出。不得把功能已接入登记为已完成或已验收。\n";
    p
}

fn catalog() {
    [
        #{
            id: "S2-01",
            wave: "s2-consumers",
            title: "M01 客户接入 DataScope v2",
            contract: "docs/organization-data-scope-s2.md 第 4 章 S2-01；上位合同第 5、8、9 章",
            template: "backend/crates/erp-read-models/src/sales_center/access.rs 与 order/scope.rs",
            owns: "backend/crates/erp-customer；backend/apps/web-api/src/core/handler 客户入口；erp-client/features/customers；docs/organization-data-scope-s2.md 的 S2-01 行",
            must: "列表/详情/全量授权/导出/写命令与归属变更都走 DataScopeService；主责与协作分别解释；org_unit_ids 按当前主负责人所属组织筛选；返回 scope_summary/as_of/policy_version/organization_version/scope_version；跨页缺版本或版本变化返回 DATA_SCOPE_CHANGED；CSV 完整结果且最后一页后再撤权重验；前端 URL、Query key、无范围/筛选空/失败三分、窄屏。",
            gap: "功能已接入 v2；剩余为 S2-12 的 A34／A36 与业务验收，不得登记本项整体完成。自动选批应跳过，除非显式传入 S2-01。",
            forbids: "不得开放销售改派；不得缺范围补 Company；不得把创建人当主责；不得改客户归属自然日规则；不得跑真实 MongoDB。",
            tests: "定向：erp-customer --lib；web-api 客户 Query 解码；erp-client features/customers 与 data-scope。禁止 cargo test --workspace、clippy --workspace。",
        },
        #{
            id: "S2-02",
            wave: "s2-consumers",
            title: "M02 合同接入 DataScope v2",
            contract: "docs/organization-data-scope-s2.md S2-02；上位合同第 6.1 合同行、第 8 章",
            template: "backend/crates/erp-read-models/src/sales_center/access.rs",
            owns: "backend/crates/erp-contract；web-api 合同 handler；erp-client/features/contracts；S2-02 文档行",
            must: "当前跟进负责人=客户当前主负责人；合法单据参与可读；组织筛选、附件、详情、候选、导出独立重验；统一范围元数据与跨页/导出版本。签约经办只作历史字段。",
            gap: "合同有 S1 owner_user_ids，未接 v2 解析器，无组织筛选与 scope_version。",
            forbids: "不得用签约人兜底当前负责人；不得静默忽略已废弃姓名参数；不得改审批节点。",
            tests: "erp-contract --lib；合同前端 vitest。禁止全仓 clippy/test。",
        },
        #{
            id: "S2-04",
            wave: "s2-consumers",
            title: "M05 采购接入 DataScope v2",
            contract: "docs/organization-data-scope-s2.md S2-04",
            template: "backend/crates/erp-read-models/src/sales_center/access.rs；erp-sales/src/repository/sales_order/scope.rs",
            owns: "backend/crates/erp-procurement；erp-read-models/src/purchase_center；web-api 采购 handler；erp-client/features/purchase-orders；S2-04 文档行",
            must: "已采集的 business_org_unit_id 与当前采购负责人进入 v2 范围；列表/详情/候选/导出/写命令消费同一解析器；范围元数据与跨页/导出撤权重验。",
            gap: "采购已写业务组织，范围消费未完成。",
            forbids: "不得改采购改派级联；不得把仓库 ID 与部门 ID 放进同一并集。",
            tests: "erp-procurement --lib；erp-read-models 采购相关；purchase-orders vitest。",
        },
        #{
            id: "S2-03",
            wave: "s2-sales",
            title: "M04 销售剩余：组织筛选、关联对象、变更单",
            contract: "docs/organization-data-scope-s2.md S2-03；s3.md S3-01 未核销部分",
            template: "sales_center/order/scope.rs；sales_center/access.rs",
            owns: "erp-sales；erp-read-models/src/sales_center；erp-processes/src/sales_change；erp-client/features/sales-orders；S2-03 文档行",
            must: "销售列表启用 org_unit_ids/include_descendants（当前 business_org_unit_id）；关联客户/合同读取走 v2；变更单沿原单业务归属且独立重验；附件与独立打印消费者重验。当前 handler 测试把 org_unit_ids 当未接入参数拒绝，必须改为合法筛选。",
            gap: "主单列表/详情/CSV/命令已接入；组织筛选、关联客户合同、变更单、任务命令未完成。",
            forbids: "不得开放销售责任改派；不得改历史归属快照；不得把任务命令混进本批（任务归 S2-10）。",
            tests: "erp-sales --lib；erp-read-models 销售；sales-orders vitest。",
        },
        #{
            id: "S2-08",
            wave: "s2-ui",
            title: "组织页面、影响预览、范围配置",
            contract: "docs/organization-data-scope-s2.md S2-08；上位合同第 4.3、8.2 节",
            template: "erp-client/features/admin 与 access-audit 页面模式；backend/apps/web-api/src/core/handler/organization/mod.rs",
            owns: "erp-client/features/organization；相关路由；必要时 identity DTO 展示字段；S2-08 文档行",
            must: "组织树、成员调岗、管理授权、变更前后影响预览、提交重验 expected_version；范围配置按资源/动作，禁止目标通配与显示名身份；URL 与 Query 缓存同步；390px 无横向溢出。组织配置权不授予业务执行权。",
            gap: "后端 GET/preview/change 已有；前端 features/organization/pages 为空。",
            forbids: "不得因部门负责人身份授予 org_unit:manage；不得自动改派任务。",
            tests: "组织前端 vitest；tsc 本功能。",
        },
        #{
            id: "S2-09",
            wave: "s2-import",
            title: "导入与商城责任映射",
            contract: "docs/organization-data-scope-s2.md S2-09；上位合同第 6.1 条 4、第 10 章条 5",
            template: "erp-processes/src/business_ownership.rs；现有 import-opening 异常处理",
            owns: "导入/商城同步入口；business_ownership；异常处理；S2-09 文档行",
            must: "ERP 建单以外的导入与商城入口使用明确责任映射；缺映射进现有异常，不生成缺负责人和业务组织的正式单；不得把系统账号映射成销售人员；首次生效来源可证明。",
            gap: "仅 ERP 建单采集组织；导入/商城未交付。",
            forbids: "不得用创建人或同步执行人兜底销售负责人。",
            tests: "触及 crate 的 --lib 定向测试。",
        },
        #{
            id: "S2-05",
            wave: "s2-adapters",
            title: "替换旧 DataScope 读取器与工作流适配器",
            contract: "docs/organization-data-scope-s2.md S2-05；上位合同第 5.1 条 2",
            template: "erp-identity/src/service/access_control/resolve.rs",
            owns: "erp-processes/src/adapters/workflow/authorization.rs；erp-read-models/src/workbench/access.rs；仍按无资源动作解释旧覆盖的读取器",
            must: "全部既有 DataScope 读取器按资源、动作、身份维度解释 v2；禁止把无资源动作的旧覆盖计算当作 v2 结果；缺动作拒绝，缺范围空集。",
            gap: "工作台与工作流适配器仍读旧 DataScope.scope_targets。",
            forbids: "不得改变任务分派规则；不得新增团队责任池。",
            tests: "erp-read-models workbench；erp-processes 适配器定向测试。",
        },
        #{
            id: "S2-10",
            wave: "s2-tasks",
            title: "任务管理边界与改派候选 A21-A25",
            contract: "上位合同第 6.3 节；验收 A21-A25",
            template: "erp-read-models/src/workbench/access.rs；workbench-workitem-contract.md",
            owns: "workbench 查询与改派候选；erp-workflow 改派入口；erp-client work-items/workspace",
            must: "管理可见不等于 work_item:manage；改派候选用任务专用资格查询；同部门无资格不得接收，跨部门有资格不得仅因部门排除；采购履约级联保持原子；调岗不自动改派，管理视图显示受阻。查询不写任务。",
            gap: "组织接入后的任务资格未验证；无 handler_user_ids 管理筛选合同。",
            forbids: "不得改审批运行时改派禁令；不得以筛选测试代替改派验收。",
            tests: "workbench/workflow 定向单测。真实账号矩阵标未执行。",
        },
        #{
            id: "S3-02",
            wave: "s3-catalog",
            title: "M03 选品册与销售方案责任",
            contract: "docs/organization-data-scope-s3.md S3-02；上位合同第 3 章 M03、第 6.1 选品册行",
            template: "销售单 sales_owner_user_id 与 SalesAccess",
            owns: "选品册/方案领域；erp-client/features/sales-selection；预定义范围清单增补",
            must: "册建立显式销售责任与业务组织；方案继承册；列表/详情/候选/导出/命令走 v2；客户提交人不是内部销售负责人。",
            gap: "M03 仍依赖客户范围入口，无显式销售责任。",
            forbids: "不得只加人员筛选项就登记完成。",
            tests: "选品相关 crate --lib；sales-selection vitest。",
        },
        #{
            id: "S3-03a",
            wave: "s3-funds",
            title: "M07 客户往来：分配金额与部分读取",
            contract: "s3.md S3-03；上位合同第 7 章、A11/A29",
            template: "erp-read-models/src/finance/cost.rs 部分读取",
            owns: "erp-finance 应收/收款；erp-read-models/src/finance/receivable；erp-client/features/customer-receivables",
            must: "负责销售、登记/核销经办人分别查询；人员汇总只算匹配份额；部分授权不返回整单合计、其他分配或完整凭证；未分配单列；整单读取须独立资格。",
            gap: "资金模块未接 v2，无部分/整单登记。",
            forbids: "不得客户端按单据头重算；不得用零值掩盖受限金额。",
            tests: "finance/receivable 定向测试；customer-receivables vitest。",
        },
        #{
            id: "S3-03b",
            wave: "s3-funds",
            title: "M08 开票申请与开票记录",
            contract: "s3.md S3-03；sales-invoice-request-contract.md 保持正式开票准入",
            template: "成本部分读取与销售范围求交",
            owns: "开票申请读模型与命令；erp-client/features/invoice-requests",
            must: "负责销售、申请人、当前开票处理人分别查询；范围与部分/整单规则同第 7 章。",
            gap: "开票无分角色人员条件与 v2 范围。",
            forbids: "不得改变正式开票准入。",
            tests: "开票相关 --lib 与前端测试。",
        },
        #{
            id: "S3-03c",
            wave: "s3-funds",
            title: "M09 供应商往来",
            contract: "s3.md S3-03；上位合同第 7 章",
            template: "M07 同一金额规则，采购负责人口径",
            owns: "erp-finance 应付/付款；erp-read-models/src/finance/payable；erp-client/features/supplier-payables",
            must: "采购负责人、付款/收票经办人分别查询；分配与部分读取同第 7 章。",
            gap: "应付未接 v2。",
            forbids: "未定义收入分摊前不得按采购人员分摊销售收入排名。",
            tests: "payable 定向测试；supplier-payables vitest。",
        },
        #{
            id: "S3-04",
            wave: "s3-workbench",
            title: "M06 履约验收与 M12 工作台管理查询",
            contract: "s3.md S3-04；上位合同 M06/M12、第 6.3 节",
            template: "S2-10 任务资格；fulfillment-responsibility-contract.md",
            owns: "erp-read-models/src/workbench；fulfillment_center；erp-client/features/fulfillment-operations 与 workspace",
            must: "处理人查询、管理范围、来源销售/采购维度、受阻任务展示；独立改派候选；原有原子级联保持；查看他人待办不显示代办按钮。",
            gap: "工作台仍旧范围；无 handler_user_ids 合同。",
            forbids: "不得新增第二套任务入口；不得自动改派审批任务。",
            tests: "workbench/fulfillment 定向测试。",
        },
        #{
            id: "S3-05",
            wave: "s3-quality",
            title: "M10 客户经营质量双口径",
            contract: "s3.md S3-05；上位合同第 6.2 条 5",
            template: "actual_profit_loss 历史归属 vs 当前责任分离",
            owns: "客户经营质量读模型；erp-client/features/customer-quality",
            must: "当前负责客户的经营情况与历史负责订单贡献分开列表/汇总/下钻/导出；禁止混入同一排名。",
            gap: "页面无双口径与归属筛选。",
            forbids: "不得用现任负责人回填历史贡献。",
            tests: "customer-quality 后端与前端定向测试。",
        },
        #{
            id: "S3-07",
            wave: "s3-handover",
            title: "销售责任交接与验收联动",
            contract: "s3.md S3-07；上位合同第 6.1 条 6、第 6.3 条 9、A26",
            template: "客户交接列出未结单据；采购责任级联事务",
            owns: "销售交接命令；验收任务责任来源；相关前端；履约合同同步点",
            must: "显式交接命令；目标资格；开放验收任务原子处理；新任务来源切换；审批与历史贡献不变；幂等审计。完成本批前不得开放会造成销售负责人与验收任务不一致的路径。",
            gap: "未实施。S2 明确禁止因业务组织字段开放销售改派。",
            forbids: "不得改开放审批任务、已完成验收、历史归属快照。",
            tests: "交接命令实体与服务测试；标明浏览器/真实库未执行。",
        },
        #{
            id: "S4-13",
            wave: "s4-master",
            title: "M13 供应商责任事实与查询",
            contract: "上位合同第 3 章 M13、条 4",
            template: "客户 assignment 有效期模式",
            owns: "供应商维护/供给能力负责人；主数据供应商页面",
            must: "先补责任创建、交接与审计，再开放人员筛选；不得用创建人或外部联系人代替内部负责人。",
            gap: "S4 未开始。",
            forbids: "不得虚构负责人。",
            tests: "供应商领域定向测试。",
        },
        #{
            id: "S4-14",
            wave: "s4-master",
            title: "M14 商品与供给负责人",
            contract: "上位合同 M14",
            template: "采购责任解析器",
            owns: "商品/供给维护负责人；由采购规则解析的采购负责人",
            must: "两类负责人独立查询；范围走 v2。",
            gap: "S4 未开始。",
            forbids: "不得用最近修改人代替维护负责人。",
            tests: "商品/供给定向测试。",
        },
        #{
            id: "S4-15",
            wave: "s4-supply",
            title: "M15 API 供应商订单",
            contract: "上位合同 M15",
            template: "销售当前负责人 + 处理人分离",
            owns: "erp-client/features/supplier-orders；对应读模型",
            must: "内部跟进负责人与异常处理人分别查询。",
            gap: "S4 未开始。",
            forbids: "不得把外部供应商联系人当内部负责人。",
            tests: "supplier-orders 定向测试。",
        },
        #{
            id: "S4-16",
            wave: "s4-supply",
            title: "M16 供应商结算",
            contract: "上位合同 M16",
            template: "资金部分读取",
            owns: "erp-client/features/supplier-settlements；settlement 读模型",
            must: "对账负责人、差异处理人、实际复核人分别查询；先有责任事实再筛选。",
            gap: "S4 未开始。",
            forbids: "不得重复计算金额。",
            tests: "settlement 定向测试。",
        },
        #{
            id: "S4-17",
            wave: "s4-ops",
            title: "M17 集成异常与业务差异",
            contract: "上位合同 M17",
            template: "工作台当前/历史处理人",
            owns: "erp-client/features/integration-errors；integration_center",
            must: "当前处理人、历史处理人查询；范围走 v2。",
            gap: "S4 未开始。",
            forbids: "不得用创建人代替处理人。",
            tests: "integration 定向测试。",
        },
        #{
            id: "S4-18",
            wave: "s4-ops",
            title: "M18 库存流水与调整经办",
            contract: "上位合同 M18、第 3 章条 2",
            template: "经办人/申请人/当前审批人分离",
            owns: "erp-client/features/inventory；库存调整命令",
            must: "实际经办人、申请人、当前审批人分别查询；库存余额不建立虚构所有人。",
            gap: "S4 未开始。",
            forbids: "不得把部门 ID 写入仓库字段。",
            tests: "inventory 定向测试。",
        },
    ]
}

fn known_waves() {
    [
        "inventory",
        "s2-consumers",
        "s2-sales",
        "s2-ui",
        "s2-import",
        "s2-adapters",
        "s2-tasks",
        "s3-catalog",
        "s3-funds",
        "s3-workbench",
        "s3-quality",
        "s3-handover",
        "s4-master",
        "s4-supply",
        "s4-ops",
    ]
}

fn next_wave_name(wave) {
    if wave == "s2-consumers" {
        "s2-sales"
    } else if wave == "s2-sales" {
        "s2-ui"
    } else if wave == "s2-ui" {
        "s2-import"
    } else if wave == "s2-import" {
        "s2-adapters"
    } else if wave == "s2-adapters" {
        "s2-tasks"
    } else if wave == "s2-tasks" {
        "s3-catalog"
    } else if wave == "s3-catalog" {
        "s3-funds"
    } else if wave == "s3-funds" {
        "s3-workbench"
    } else if wave == "s3-workbench" {
        "s3-quality"
    } else if wave == "s3-quality" {
        "s3-handover"
    } else if wave == "s3-handover" {
        "s4-master"
    } else if wave == "s4-master" {
        "s4-supply"
    } else if wave == "s4-supply" {
        "s4-ops"
    } else {
        "done"
    }
}

fn standards_block() {
    let p = "";
    p += "编码与门禁（必须遵守）：\n";
    p += "1. 先读代码再改。分层 HTTP Handler -> Service -> Repository -> MongoDB。\n";
    p += "2. 新增/修改的每个 Rust 方法（含私有）必须有多行文档注释：用途、参数、返回、错误、关键约束。\n";
    p += "3. 新能力落到职责文件（scope.rs/query.rs/access.rs），禁止追加到巨型 mod.rs。\n";
    p += "4. Repository 只收明确授权条件与 executor；禁止按登录用户自行推断权限。\n";
    p += "5. 缺动作=403；缺范围=空集；不得 Company 兜底；历史参与只用于读取。\n";
    p += "6. 禁止真实 MongoDB：测试必须 env -u ERP_TEST_MONGO_URI。禁止 --ignored。\n";
    p += "7. 禁止 cargo fmt --all、clippy --workspace、cargo test --workspace 作为本批门禁。只用定向 -p crate --lib 与功能 vitest。\n";
    p += "8. 不得开放销售责任改派，除非批次是 S3-07。不得改审批指定到人。\n";
    p += "9. 只更新本批在 docs/organization-data-scope-s2.md 或 s3.md 中对应行的状态；不得把阶段改成已验收或已完成。\n";
    p += "10. 前端跟 sales-orders 的 scopeVersion、emptyReason、data-scope/cache.ts。筛选进 URL 与 Query key。\n";
    p += "11. 提交前在仓库根目录执行 bash backend/scripts/check-org-data-scope.sh（合同 docs/organization-data-scope-gate.md）。退出码必须为 0 且 JSON status 为 STATIC_CHECKS_PASSED。禁止 --self-test-only 充当放行；禁止忽略路径、债务白名单、把失败改警告。待接入项与整改项均阻断，必须改生产代码接入公共解析。本检查只证明第 9 章静态门禁，不得登记功能或业务验收。不启动服务、不跑真实 MongoDB。\n";
    p
}

fn implement_prompt(batch, land, status_row) {
    let p = "";
    p += "你正在实现组织数据范围合同批次 ";
    p += batch.id;
    p += "：";
    p += batch.title;
    p += "。必须用 read_file/grep/list_dir 阅读真实代码，禁止凭记忆编写。\n\n";
    p += "必读：\n";
    p += "- docs/organization-data-scope-contract.md 第 5、8、9 章与本批相关功能组\n- ";
    p += batch.contract;
    p += "\n- 模板：";
    p += batch.template;
    p += "\n- backend/crates/erp-identity/src/service/access_control/resolve.rs\n";
    p += "- backend/crates/erp-identity/src/service/iam/predefined_data_scopes.rs\n";
    p += "- ~/.claude/skills/rust-coding-standards/SKILL.md 与 references/file-organization.md\n\n";
    p += "范围 owns：";
    p += batch.owns;
    p += "\n必须交付：";
    p += batch.must;
    p += "\n合同当前状态：";
    p += status_field(status_row, "contract_status", "（未读到合同行，以下列编目缺口为准）");
    p += "\n本批剩余：";
    p += status_field(status_row, "remaining", batch.gap);
    p += "\n编目缺口备注：";
    p += batch.gap;
    p += "\n禁止：";
    p += batch.forbids;
    p += "\n测试：";
    p += batch.tests;
    p += "\n";
    if status_field(status_row, "disposition", "") == "partial" {
        p += "本批已有部分接入。只补“本批剩余”中的代码缺口，禁止重写已符合合同的实现。\n";
    }
    if status_field(status_row, "disposition", "") == "blocked_acceptance" {
        p += "本批功能已接入，剩余主要是验收。不要推倒重来；只补仍缺的生产结构。A34/A36/真实库/浏览器写入 still_open，不得登记已验收。\n";
    }
    p += "以合同表格与当前代码为准，不得用过期编目描述覆盖已经接入的事实。\n\n";
    p += standards_block();
    p += "\n工作区是 ERP 仓库根目录。";
    if land {
        p += "直接在当前工作区改文件，不要 isolation。改完后 git add 本批文件并创建提交，message 用 feat(org-scope): ";
        p += batch.id;
        p += "。若用户工作区策略禁止提交，则 committed=false 并说明。\n";
    } else {
        p += "你在隔离 worktree。在本 worktree 提交到分支 org-scope/";
        p += batch.id;
        p += "，不要合并到 main，不要改 owns 之外的无关文件。\n";
    }
    p += "完成后返回 JSON：batch_id、completed、committed、commit_sha、branch、files_changed、commands、summary、blockers、still_open。\n";
    p += "只有合同要求的本批可实施行为都有代码证据才 completed=true。环境限制（无 Mongo、无浏览器真实账号）写入 still_open，不要因此 completed=false。不得把阶段改成已验收。\n";
    p
}

fn review_prompt(batch, status_row) {
    let p = "";
    p += "对抗评审批次 ";
    p += batch.id;
    p += "。阅读实现后的代码，不要相信实现者摘要。\n";
    p += "对照：";
    p += batch.contract;
    p += "\n必须交付：";
    p += batch.must;
    p += "\n合同当前状态：";
    p += status_field(status_row, "contract_status", "");
    p += "\n本批剩余：";
    p += status_field(status_row, "remaining", batch.gap);
    p += "\n禁止项若被违反，approved=false。\n";
    p += "检查：v2 解析是否按资源动作；空范围是否空集；查询元数据；跨页版本；前端是否消费；测试是否覆盖失败路径；文件是否落到职责模块。\n";
    if status_field(status_row, "disposition", "") == "partial" {
        p += "已接入部分不得因风格偏好被判 blocker。只对 remaining 缺口和禁止项失败。\n";
    }
    p += "approved=true 仅当无 blocker 级 findings。evidence 必须引用文件路径。不得因未跑真实库/A34 数据库等价而否决本批代码交付。\n";
    p
}

fn repair_prompt(batch, findings_json) {
    let p = "";
    p += "修复批次 ";
    p += batch.id;
    p += " 的评审 findings。只修本批 owns 内问题。\nfindings JSON：";
    p += findings_json;
    p += "\n";
    p += standards_block();
    p += "返回与实现阶段相同的 JSON schema。\n";
    p
}

fn verify_prompt(batch) {
    let p = "";
    p += "验证批次 ";
    p += batch.id;
    p += "。运行测试：";
    p += batch.tests;
    p += "\n在 backend/ 跑定向 cargo test；在 erp-client/ 跑定向 vitest。\n";
    p += "禁止 ERP_TEST_MONGO_URI、--ignored、全仓 clippy/test。\n";
    p += "用 grep 确认 DataScopeService 或本批要求的查询参数确实被调用，而不是只改文档。\n";
    p += "all_pass=true 仅当定向命令成功且关键入口有证据。环境缺失标 environment_blocked。\n";
    p
}

fn gate_run_prompt(wave, target_id, batch_csv) {
    let p = "";
    p += "执行组织与数据范围第 9 章静态门禁。这是质量放行依据，不是自检演示。\n";
    p += "必读 docs/organization-data-scope-gate.md 第 1–3 节。\n";
    p += "当前 wave=";
    p += wave;
    p += " target=";
    p += target_id;
    p += " batches=";
    p += batch_csv;
    p += "\n\n必须用 run_terminal_command 在仓库根目录真实执行：\n";
    p += "bash backend/scripts/check-org-data-scope.sh --json\n";
    p += "不要 cd 到无关目录。JSON 在 stdout，规则自检与领域门禁日志在 stderr。记录该命令的退出码。\n\n";
    p += "禁止：\n";
    p += "- 使用 --self-test-only（仅工具自检，不得作为工作区放行）\n";
    p += "- 未执行命令就填 passed=true\n";
    p += "- 把 BLOCKED / TOOL_ERROR / SELF_TEST_ONLY 当成通过\n";
    p += "- 启动服务、连接 MongoDB、跑数据库集成测试\n";
    p += "- 修改检查器、添加忽略路径、债务白名单、把失败改警告\n\n";
    p += "passed=true 当且仅当：你确实运行了上述命令，exit_code==0，且 JSON status 恰好是 STATIC_CHECKS_PASSED。\n";
    p += "exit_code=1 为源码规则或既有领域门禁阻断；exit_code=2 为参数/工具/自检/Cargo metadata/源码读取失败。\n";
    p += "findings 必须逐条抄录 JSON：rule、path、line、message、category（整改项或待接入项）。待接入项同样阻断。\n";
    p += "not_proven 抄录 JSON 的 not_proven；这些不是本门禁通过条件，报告中保留。\n";
    p += "command 填实际执行的命令。summary 用一句话说明状态、扫描文件数、阻断条数。\n";
    p += "若命令无法执行：passed=false，exit_code=2，status=TOOL_ERROR，findings=[]，summary 写原因。\n";
    p
}

fn gate_fix_prompt(wave, target_id, batch_csv, findings_json, usable) {
    let p = "";
    p += "第 9 章静态门禁未通过，必须改生产代码直到 bash backend/scripts/check-org-data-scope.sh 退出码 0 且 status=STATIC_CHECKS_PASSED。\n";
    p += "必读：docs/organization-data-scope-gate.md、docs/organization-data-scope-contract.md 第 9 章。\n";
    p += "wave=";
    p += wave;
    p += " target=";
    p += target_id;
    p += " batches=";
    p += batch_csv;
    p += "\n\n";
    if usable {
        p += "本轮门禁 findings JSON：";
        p += findings_json;
        p += "\n";
    } else {
        p += "上一轮门禁运行结果不可用。你必须先亲自执行 bash backend/scripts/check-org-data-scope.sh --json，再按真实 findings 整改。\n";
    }
    p += "\n整改规则：\n";
    p += "1. 待接入项与整改项均不得豁免。禁止忽略路径、债务白名单、把失败改警告、改门禁脚本放行、用 --self-test-only 充当通过。\n";
    p += "2. 按规则修复：ODS-DOMAIN 修领域边界；ODS-OWNER/ENGINE 把范围类型与解析器留在身份域；ODS-PORT 从 Port 去掉身份内部类型；ODS-RAW 停止解释 scope_type/scope_targets，改为调用公共解析；ODS-REPOSITORY 禁止业务仓储加载授权；ODS-ADAPTER 在同一语句 DataScopeService::new(...).resolve/resolve_permissions 并传递 executor；ODS-EXECUTOR 禁止 NoTransaction/自开事务；ODS-FALLBACK 禁止 unwrap_or* 与 company:true / authorized_customer_ids:None 默认成功；ODS-REGISTRY/DIMENSIONS 去掉 RESOURCE_ACTIONS 准入和固定单一维度；ODS-SAMPLE 恢复客户样例生产结构。\n";
    p += "3. 不得以更名、拆分语句、宏包装、改 cfg 规避规则。adapter 若改为字段注入等结构，必须同步扩展门禁正反例，不得关闭检查。\n";
    p += "4. 修复范围为使静态门禁通过的最小生产改动；不要新开未选中的功能批次，不要做第 4 节行为验收，不要启动服务或真实 MongoDB。\n";
    p += "5. 改完后必须再跑 bash backend/scripts/check-org-data-scope.sh --json。fixed=true 仅当这次重跑 exit_code==0 且 status=STATIC_CHECKS_PASSED。remaining_findings 抄录仍存在的阻断；若已通过则为 []。\n";
    p += standards_block();
    p
}

fn inventory_report(all_batches, status_by_id, s2_stage, s3_stage, s4_stage, usable) {
    let md = "";
    md += "# 组织数据范围剩余批次\n\n";
    md += "本报告由 workflow implement-org-data-scope 生成。先读合同状态，再对照 catalog。\n\n";
    md += "- S2 阶段：";
    md += s2_stage;
    md += "\n- S3 阶段：";
    md += s3_stage;
    md += "\n- S4 阶段：";
    md += s4_stage;
    md += "\n- 合同状态完整覆盖 catalog：";
    md += json_encode(usable);
    md += "\n\n| 批次 | 波次 | 分类 | 合同状态 | 剩余 |\n| --- | --- | --- | --- | --- |\n";
    for batch in all_batches {
        let row = status_by_id[batch.id];
        md += "| ";
        md += batch.id;
        md += " | ";
        md += batch.wave;
        md += " | ";
        md += status_field(row, "disposition", "unknown");
        md += " | ";
        md += status_field(row, "contract_status", batch.title);
        md += " | ";
        md += status_field(row, "remaining", batch.gap);
        md += " |\n";
    }
    md += "\n自动开做：disposition 为 open 或 partial。跳过 blocked_acceptance、out_of_scope。S2 未关闭前不自动进入 S3/S4。\n";
    md += "推荐顺序：s2-consumers → s2-sales → s2-ui → s2-import → s2-adapters → s2-tasks → s3-catalog → s3-funds → s3-workbench → s3-quality → s3-handover → s4-*。\n";
    md += "不在 workflow 内执行：S2-11 真实库/初始化重跑/索引计划、第 10 章上线准入、A 矩阵业务验收。\n";
    md
}

// ---- args ----
let wave = "";
let requested_ids = [];
let land_arg = ();
let max_gate_rounds = 8;
if args != () {
    if args.wave != () {
        wave = args.wave;
    }
    if args.batch_ids != () {
        requested_ids = args.batch_ids;
    }
    if args.land != () {
        land_arg = args.land;
    }
    if args.max_gate_rounds != () {
        max_gate_rounds = args.max_gate_rounds;
    }
}
if max_gate_rounds < 1 {
    max_gate_rounds = 1;
}
if max_gate_rounds > 16 {
    max_gate_rounds = 16;
}
if is_text(wave) && !contains_text(known_waves(), wave) {
    pause("verification", "Unknown args.wave. Use one of the documented wave names, or omit wave to auto-select.");
}

let all_batches = catalog();
let catalog_ids = [];
let catalog_id_set = #{};
for batch in all_batches {
    catalog_ids.push(batch.id);
    catalog_id_set[batch.id] = true;
}

phase("Select");
let status_agent = agent(contract_status_prompt(json_encode(catalog_ids)), #{
    label: "contract-status",
    capability_mode: "read-only",
    output_schema: contract_status_schema,
});

let status_by_id = #{};
let status_rows = [];
let s2_stage = "unknown";
let s3_stage = "unknown";
let s4_stage = "unknown";
if status_agent != () && status_agent.success && is_map(status_agent.output) {
    if is_text(status_agent.output.s2_stage) {
        s2_stage = status_agent.output.s2_stage;
    }
    if is_text(status_agent.output.s3_stage) {
        s3_stage = status_agent.output.s3_stage;
    }
    if is_text(status_agent.output.s4_stage) {
        s4_stage = status_agent.output.s4_stage;
    }
    if is_arr(status_agent.output.batches) {
        for row in status_agent.output.batches {
            if is_map(row) && is_text(row.batch_id) && catalog_id_set[row.batch_id] == true && is_allowed_disposition(row.disposition) {
                status_by_id[row.batch_id] = row;
                status_rows.push(row);
            } else if is_map(row) && is_text(row.batch_id) {
                log("dropped contract-status row " + row.batch_id);
            }
        }
    }
}

let status_covered = 0;
for batch in all_batches {
    if status_by_id[batch.id] != () {
        status_covered += 1;
    }
}
let status_usable = status_covered == all_batches.len();
log("contract-status usable=" + json_encode(status_usable) + " covered=" + status_covered.to_string() + "/" + all_batches.len().to_string() + " S2=" + s2_stage + " S3=" + s3_stage + " S4=" + s4_stage);
write_scratch_file("contract-status.json", json_encode(#{
    s2_stage: s2_stage,
    s3_stage: s3_stage,
    s4_stage: s4_stage,
    usable: status_usable,
    batches: status_rows,
}));

if wave == "inventory" {
    let path = write_scratch_file("remaining-batches.md", inventory_report(all_batches, status_by_id, s2_stage, s3_stage, s4_stage, status_usable));
    complete(#{ path: path, wave: "inventory", next_wave: "s2-consumers", gate_passed: false, s2_stage: s2_stage, s3_stage: s3_stage });
}

let auto_wave = false;
if requested_ids.len() > 0 && !is_text(wave) {
    for batch in all_batches {
        if contains_text(requested_ids, batch.id) && !is_text(wave) {
            wave = batch.wave;
        }
    }
}
if !is_text(wave) {
    auto_wave = true;
    if !status_usable {
        pause("verification", "合同状态未完整覆盖 catalog。请传入 args.wave（如 s2-consumers），或重跑以读取 S2/S3 合同表。");
    }
    let target_stage = "S2";
    if stage_closed(s2_stage) {
        if stage_closed(s3_stage) {
            target_stage = "S4";
        } else {
            target_stage = "S3";
        }
    }
    for w in known_waves() {
        if w != "inventory" && stage_of_wave(w) == target_stage && !is_text(wave) {
            for batch in all_batches {
                if batch.wave == w && is_auto_eligible(status_by_id[batch.id]) && !is_text(wave) {
                    wave = w;
                }
            }
        }
    }
    if !is_text(wave) {
        let empty_path = write_scratch_file("org-data-scope-wave-report.md", "# 无自动可实施批次\n\n阶段 S2=" + s2_stage + " S3=" + s3_stage + " S4=" + s4_stage + "。open/partial 已空，或剩余仅为验收/范围外。显式传入 args.wave 或 args.batch_ids。\n");
        complete(#{ path: empty_path, wave: "", selected: [], skipped: catalog_ids, next_wave: "s2-consumers", gate_passed: false });
    }
    log("auto-selected wave=" + wave + " stage=" + target_stage);
}

let selected = [];
let skipped_notes = [];
let deferred_ids = [];
if requested_ids.len() > 0 {
    for batch in all_batches {
        if contains_text(requested_ids, batch.id) {
            if is_text(wave) && batch.wave != wave {
                skipped_notes.push(batch.id + " not in wave " + wave);
            } else {
                selected.push(batch);
            }
        }
    }
} else {
    for batch in all_batches {
        if batch.wave == wave {
            if !status_usable {
                selected.push(batch);
            } else if is_auto_eligible(status_by_id[batch.id]) {
                selected.push(batch);
            } else {
                let row = status_by_id[batch.id];
                skipped_notes.push(batch.id + " " + status_field(row, "disposition", "unknown") + ": " + status_field(row, "remaining", batch.gap));
            }
        }
    }
}

if selected.len() > 3 {
    let kept = [];
    let i_sel = 0;
    for batch in selected {
        if i_sel < 3 {
            kept.push(batch);
        } else {
            deferred_ids.push(batch.id);
            skipped_notes.push(batch.id + " deferred; at most 3 batches per run");
        }
        i_sel += 1;
    }
    selected = kept;
}

if selected.len() == 0 {
    let none_md = "";
    none_md += "# 本波次无自动可实施批次\n\nwave=";
    none_md += wave;
    none_md += "\n\n";
    for note in skipped_notes {
        none_md += "- ";
        none_md += note;
        none_md += "\n";
    }
    none_md += "\n显式传入 args.batch_ids 可强制开做（仍按合同剩余缺口补，不重写已接入部分）。\n";
    let none_path = write_scratch_file("org-data-scope-wave-report.md", none_md);
    complete(#{ path: none_path, wave: wave, selected: [], skipped: skipped_notes, next_wave: next_wave_name(wave), gate_passed: false });
}

let land = selected.len() == 1;
if land_arg != () {
    land = land_arg;
}

log("wave=" + wave + " auto=" + json_encode(auto_wave) + " batches=" + json_encode(selected.len()) + " land=" + json_encode(land));

// ---- implement ----
phase("Implement");
let impl_results = [];
if land {
    for batch in selected {
        let r = agent(implement_prompt(batch, true, status_by_id[batch.id]), #{
            label: "impl:" + batch.id,
            capability_mode: "all",
            output_schema: implementation_schema,
        });
        impl_results.push(r);
    }
} else {
    let jobs = [];
    for batch in selected {
        jobs.push(#{
            prompt: implement_prompt(batch, false, status_by_id[batch.id]),
            label: "impl:" + batch.id,
            capability_mode: "all",
            isolation_worktree: true,
            output_schema: implementation_schema,
        });
    }
    impl_results = parallel(jobs);
}

let impl_ok = [];
let impl_ids = [];
let i = 0;
for batch in selected {
    let r = impl_results[i];
    let ok = r != () && r.success && is_map(r.output) && r.output.completed == true && r.output.batch_id == batch.id;
    impl_ok.push(ok);
    if r != () && is_text(r.agent_id) {
        impl_ids.push(r.agent_id);
    } else {
        impl_ids.push("");
    }
    i += 1;
}

// ---- review ----
phase("Review");
let review_jobs = [];
i = 0;
for batch in selected {
    if impl_ok[i] {
        let opts = #{
            prompt: review_prompt(batch, status_by_id[batch.id]),
            label: "review:" + batch.id,
            capability_mode: "read-only",
            output_schema: review_schema,
        };
        if !land && is_text(impl_ids[i]) {
            opts.resume_from = impl_ids[i];
        }
        review_jobs.push(opts);
    }
    i += 1;
}
let review_results = [];
if review_jobs.len() > 0 {
    review_results = parallel(review_jobs);
}

let review_by_id = #{};
for r in review_results {
    if r != () && r.success && is_map(r.output) && is_text(r.output.batch_id) {
        review_by_id[r.output.batch_id] = r;
    }
}

// ---- repair ----
phase("Repair");
let repair_results = #{};
i = 0;
for batch in selected {
    let needs = false;
    let findings_json = "[]";
    if impl_ok[i] {
        let rev = review_by_id[batch.id];
        if rev == () || !rev.success || rev.output.approved != true {
            needs = true;
            if rev != () && rev.success && is_map(rev.output) {
                findings_json = json_encode(rev.output.findings);
            } else {
                findings_json = "[{\"severity\":\"blocker\",\"file\":\"(missing-review)\",\"issue\":\"review missing or failed\",\"evidence\":\"no usable review\",\"remediation\":\"fix contract gaps and re-review\"}]";
            }
        }
    }
    if needs {
        let opts = #{
            label: "repair:" + batch.id,
            capability_mode: "all",
            output_schema: implementation_schema,
        };
        if !land && is_text(impl_ids[i]) {
            opts.resume_from = impl_ids[i];
        }
        let repaired = agent(repair_prompt(batch, findings_json), opts);
        repair_results[batch.id] = repaired;
        if repaired != () && repaired.success && is_map(repaired.output) && repaired.output.completed == true {
            impl_ok[i] = true;
            if is_text(repaired.agent_id) {
                impl_ids[i] = repaired.agent_id;
            }
        }
    }
    i += 1;
}

// ---- verify ----
phase("Verify");
let verify_jobs = [];
i = 0;
for batch in selected {
    if impl_ok[i] {
        let opts = #{
            prompt: verify_prompt(batch),
            label: "verify:" + batch.id,
            capability_mode: "execute",
            output_schema: verification_schema,
        };
        if !land && is_text(impl_ids[i]) {
            opts.resume_from = impl_ids[i];
        }
        verify_jobs.push(opts);
    }
    i += 1;
}
let verify_results = [];
if verify_jobs.len() > 0 {
    verify_results = parallel(verify_jobs);
}
let verify_by_id = #{};
for r in verify_results {
    if r != () && r.success && is_map(r.output) && is_text(r.output.batch_id) {
        verify_by_id[r.output.batch_id] = r;
    }
}

// ---- chapter 9 static gate: run, fix, re-run until pass ----
phase("Gate");
let batch_csv = batch_ids_csv(selected);
let gate_target_ids = [];
let gate_resume = [];
let gate_passed_flags = [];
if land {
    gate_target_ids.push("workspace");
    gate_resume.push("");
    gate_passed_flags.push(false);
} else {
    i = 0;
    for batch in selected {
        if is_text(impl_ids[i]) {
            gate_target_ids.push(batch.id);
            gate_resume.push(impl_ids[i]);
            gate_passed_flags.push(false);
        }
        i += 1;
    }
}

let pending_idx = [];
i = 0;
while i < gate_target_ids.len() {
    pending_idx.push(i);
    i += 1;
}

let last_gate_by_id = #{};
let gate_rounds = 0;
let unusable_rounds = 0;
let last_fp_by_id = #{};

while pending_idx.len() > 0 && gate_rounds < max_gate_rounds {
    let remaining = budget().remaining;
    if remaining < pending_idx.len() {
        await_user("back_off", "剩余 agent 预算不足以继续第 9 章门禁。请提高 agent_budget 后 resume。");
        remaining = budget().remaining;
        if remaining < pending_idx.len() {
            log("gate stop: budget remaining=" + remaining.to_string());
            break;
        }
    }

    gate_rounds += 1;
    log("ods-gate round=" + gate_rounds.to_string() + " pending=" + pending_idx.len().to_string());

    let run_jobs = [];
    for idx in pending_idx {
        let opts = #{
            prompt: gate_run_prompt(wave, gate_target_ids[idx], batch_csv),
            label: "ods-gate:" + gate_target_ids[idx] + ":" + gate_rounds.to_string(),
            capability_mode: "execute",
            output_schema: gate_schema,
        };
        if is_text(gate_resume[idx]) {
            opts.resume_from = gate_resume[idx];
        }
        run_jobs.push(opts);
    }
    let run_results = parallel(run_jobs);

    let next_pending = [];
    let fix_jobs = [];
    let fix_idx = [];
    let round_all_unusable = true;
    let j = 0;
    for idx in pending_idx {
        let g = run_results[j];
        last_gate_by_id[gate_target_ids[idx]] = g;
        if is_gate_pass(g) {
            gate_passed_flags[idx] = true;
            round_all_unusable = false;
            log("ods-gate passed target=" + gate_target_ids[idx]);
        } else {
            let usable = is_gate_usable(g);
            if usable {
                round_all_unusable = false;
            }
            let fp = gate_fingerprint(g);
            let prev = last_fp_by_id[gate_target_ids[idx]];
            if prev != () && prev == fp {
                log("ods-gate unchanged findings target=" + gate_target_ids[idx]);
            }
            last_fp_by_id[gate_target_ids[idx]] = fp;
            next_pending.push(idx);
            let fix_opts = #{
                prompt: gate_fix_prompt(wave, gate_target_ids[idx], batch_csv, gate_findings_json(g), usable),
                label: "ods-gate-fix:" + gate_target_ids[idx] + ":" + gate_rounds.to_string(),
                capability_mode: "all",
                output_schema: gate_fix_schema,
            };
            if is_text(gate_resume[idx]) {
                fix_opts.resume_from = gate_resume[idx];
            }
            fix_jobs.push(fix_opts);
            fix_idx.push(idx);
        }
        j += 1;
    }

    if round_all_unusable {
        unusable_rounds += 1;
    } else {
        unusable_rounds = 0;
    }
    if unusable_rounds >= 2 {
        log("ods-gate stop: two unusable runner rounds");
        break;
    }

    pending_idx = next_pending;
    if pending_idx.len() == 0 {
        log("ods-gate all targets passed");
    } else if fix_jobs.len() > 0 {
        remaining = budget().remaining;
        if remaining < fix_jobs.len() {
            await_user("back_off", "剩余 agent 预算不足以启动第 9 章门禁整改。请提高 agent_budget 后 resume。");
            remaining = budget().remaining;
            if remaining < fix_jobs.len() {
                log("gate-fix stop: budget remaining=" + remaining.to_string());
                break;
            }
        }
        let fix_results = parallel(fix_jobs);
        let k = 0;
        for idx in fix_idx {
            let fx = fix_results[k];
            if fx != () && is_text(fx.agent_id) {
                gate_resume[idx] = fx.agent_id;
            }
            k += 1;
        }
    }
}

let all_gates_passed = all_true(gate_passed_flags);
if gate_target_ids.len() == 0 {
    all_gates_passed = false;
    log("ods-gate skipped: no targets");
} else if all_gates_passed {
    log("ods-gate STATIC_CHECKS_PASSED after " + gate_rounds.to_string() + " round(s)");
} else {
    log("ods-gate still blocked after " + gate_rounds.to_string() + " round(s)");
}

let gate_json_src = last_gate_by_id["workspace"];
if gate_json_src == () && gate_target_ids.len() == 1 {
    gate_json_src = last_gate_by_id[gate_target_ids[0]];
}
if gate_json_src != () && gate_json_src.success && is_map(gate_json_src.output) {
    write_scratch_file("org-data-scope-gate.json", json_encode(gate_json_src.output));
}

// ---- report ----
phase("Report");
let md = "";
md += "# 组织数据范围实施报告\n\n";
md += "- wave: ";
md += wave;
md += "\n- auto_wave: ";
md += json_encode(auto_wave);
md += "\n- land: ";
md += json_encode(land);
md += "\n- S2 阶段: ";
md += s2_stage;
md += "\n- S3 阶段: ";
md += s3_stage;
md += "\n- S4 阶段: ";
md += s4_stage;
md += "\n- next_wave: ";
if deferred_ids.len() > 0 {
    md += wave;
} else {
    md += next_wave_name(wave);
}
md += "\n- chapter9_static_gate: ";
if all_gates_passed {
    md += "STATIC_CHECKS_PASSED";
} else if gate_target_ids.len() == 0 {
    md += "skipped";
} else {
    md += "BLOCKED";
}
md += "\n- gate_rounds: ";
md += gate_rounds.to_string();
md += "\n- max_gate_rounds: ";
md += max_gate_rounds.to_string();
md += "\n\n| 批次 | 实现 | 评审 | 验证 | 第9章门禁 | 摘要 |\n| --- | --- | --- | --- | --- | --- |\n";

i = 0;
let completed_ids = [];
let blocked_ids = [];
for batch in selected {
    let impl_state = "failed";
    let review_state = "skipped";
    let verify_state = "skipped";
    let gate_state = "skipped";
    let summary = "";
    let r = impl_results[i];
    if land {
        if all_gates_passed {
            gate_state = "pass";
        } else if gate_target_ids.len() > 0 {
            gate_state = "fail";
        }
    } else {
        let gi = 0;
        for gid in gate_target_ids {
            if gid == batch.id {
                if gate_passed_flags[gi] == true {
                    gate_state = "pass";
                } else {
                    gate_state = "fail";
                }
            }
            gi += 1;
        }
    }
    if impl_ok[i] {
        impl_state = "ok";
        if is_map(r.output) && is_text(r.output.summary) {
            summary = r.output.summary;
        }
        if gate_state == "fail" {
            blocked_ids.push(batch.id);
        } else {
            completed_ids.push(batch.id);
        }
    } else {
        blocked_ids.push(batch.id);
        if r != () && is_map(r.output) && is_text(r.output.summary) {
            summary = r.output.summary;
        } else {
            summary = "implementer missing or schema failed";
        }
    }
    let rev = review_by_id[batch.id];
    if rev != () && rev.success && is_map(rev.output) {
        if rev.output.approved == true {
            review_state = "approved";
        } else {
            review_state = "rejected";
        }
    }
    let ver = verify_by_id[batch.id];
    if ver != () && ver.success && is_map(ver.output) {
        if ver.output.all_pass == true {
            verify_state = "pass";
        } else {
            verify_state = "fail";
        }
    }
    md += "| ";
    md += batch.id;
    md += " | ";
    md += impl_state;
    md += " | ";
    md += review_state;
    md += " | ";
    md += verify_state;
    md += " | ";
    md += gate_state;
    md += " | ";
    md += summary;
    md += " |\n";
    i += 1;
}

md += "\n## 第 9 章静态门禁\n\n";
md += "入口：`bash backend/scripts/check-org-data-scope.sh`。合同：docs/organization-data-scope-gate.md。\n";
md += "自动通过仅允许登记「第 9 章静态门禁通过」，不得据此登记功能符合合同或业务验收。\n\n";
if all_gates_passed {
    md += "结果：STATIC_CHECKS_PASSED。\n";
} else {
    md += "结果：未通过，提交批次不得质量放行。待接入项与整改项均不得豁免。\n";
}

let dumped = [];
if land {
    dumped.push("workspace");
} else {
    for batch in selected {
        dumped.push(batch.id);
    }
}
for gid in dumped {
    let g = last_gate_by_id[gid];
    if g != () && is_gate_usable(g) {
        md += "\n### ";
        md += gid;
        md += "\n\n- status: ";
        md += g.output.status;
        md += "\n- exit_code: ";
        md += json_encode(g.output.exit_code);
        md += "\n- command: `";
        md += g.output.command;
        md += "`\n";
        if is_arr(g.output.findings) && g.output.findings.len() > 0 {
            md += "\n| 规则 | 分类 | 路径 | 行 | 说明 |\n| --- | --- | --- | --- | --- |\n";
            for f in g.output.findings {
                md += "| ";
                md += f.rule;
                md += " | ";
                md += f.category;
                md += " | ";
                md += f.path;
                md += " | ";
                md += json_encode(f.line);
                md += " | ";
                md += f.message;
                md += " |\n";
            }
        }
        if is_arr(g.output.not_proven) && g.output.not_proven.len() > 0 {
            md += "\n本门禁不证明：\n";
            for item in g.output.not_proven {
                md += "- ";
                md += item;
                md += "\n";
            }
        }
    }
}

if skipped_notes.len() > 0 {
    md += "\n## 本波跳过\n\n";
    for note in skipped_notes {
        md += "- ";
        md += note;
        md += "\n";
    }
}

md += "\n未纳入本 workflow 的强制人工项：第 4 节行为核验、S2-11、第 10 章上线准入、A01-A32 真实账号与数据库验收。静态门禁通过不得登记已验收。\n";
let report_path = write_scratch_file("org-data-scope-wave-report.md", md);
let reported_next = next_wave_name(wave);
if deferred_ids.len() > 0 {
    reported_next = wave;
}
complete(#{
    path: report_path,
    wave: wave,
    auto_wave: auto_wave,
    land: land,
    completed: completed_ids,
    blocked: blocked_ids,
    skipped: skipped_notes,
    next_wave: reported_next,
    gate_passed: all_gates_passed,
    gate_rounds: gate_rounds,
    s2_stage: s2_stage,
    s3_stage: s3_stage,
});
