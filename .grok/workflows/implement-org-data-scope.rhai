// 按波次实现组织数据范围合同剩余项。
// 运行：/workflow implement-org-data-scope
// Args:
//   wave: 必填。s2-consumers | s2-sales | s2-ui | s2-import | s2-adapters | s2-tasks
//         | s3-catalog | s3-funds | s3-workbench | s3-quality | s3-handover
//         | s4-master | s4-supply | s4-ops | inventory
//   batch_ids: 可选字符串数组，只跑该波次中的指定批次
//   land: 可选布尔。true=改父工作区（多批次时顺序执行）；false=隔离 worktree 并行。
//         默认：单批次 true，多批次 false。

let meta = #{
    name: "implement-org-data-scope",
    description: "按波次实现组织数据范围合同剩余批次：S2 消费者/适配器/页面，S3 核心链路，S4 扩展。不跑真实 MongoDB，不登记业务验收。",
    when_to_use: "继续完成 organization-data-scope-contract 的剩余实现。先跑 s2-consumers，再按依赖推进。",
    phases: [
        #{ title: "Select", detail: "按 wave/batch_ids 选出仍须实现的批次" },
        #{ title: "Implement", detail: "按批次实现 DataScope v2 消费者、查询、页面与测试" },
        #{ title: "Review", detail: "对照合同与代码做对抗评审" },
        #{ title: "Repair", detail: "至多一轮修复未通过评审的批次" },
        #{ title: "Verify", detail: "定向单测与静态核对，不跑真实库" },
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
            gap: "客户仍用 CustomerScope Mine/Collaborating/AllAuthorized，未调用 DataScopeService；无 org_unit_ids、scope_version、scope_summary。",
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
    p
}

fn implement_prompt(batch, land) {
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
    p += "\n当前缺口：";
    p += batch.gap;
    p += "\n禁止：";
    p += batch.forbids;
    p += "\n测试：";
    p += batch.tests;
    p += "\n\n";
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
    p += "只有合同要求的本批行为都有代码证据才 completed=true。环境限制（无 Mongo、无浏览器真实账号）写入 still_open，不要因此 completed=false。\n";
    p
}

fn review_prompt(batch) {
    let p = "";
    p += "对抗评审批次 ";
    p += batch.id;
    p += "。阅读实现后的代码，不要相信实现者摘要。\n";
    p += "对照：";
    p += batch.contract;
    p += "\n必须交付：";
    p += batch.must;
    p += "\n禁止项若被违反，approved=false。\n";
    p += "检查：v2 解析是否按资源动作；空范围是否空集；查询元数据；跨页版本；前端是否消费；测试是否覆盖失败路径；文件是否落到职责模块。\n";
    p += "approved=true 仅当无 blocker 级 findings。evidence 必须引用文件路径。\n";
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

fn inventory_report(all_batches) {
    let md = "";
    md += "# 组织数据范围剩余批次\n\n";
    md += "本报告由 workflow implement-org-data-scope 生成，对照合同与代码入口静态编目。\n\n";
    md += "| 批次 | 波次 | 标题 |\n| --- | --- | --- |\n";
    for batch in all_batches {
        md += "| ";
        md += batch.id;
        md += " | ";
        md += batch.wave;
        md += " | ";
        md += batch.title;
        md += " |\n";
    }
    md += "\n推荐顺序：s2-consumers → s2-sales → s2-ui → s2-import → s2-adapters → s2-tasks → s3-catalog → s3-funds → s3-workbench → s3-quality → s3-handover → s4-*。\n";
    md += "不在 workflow 内执行：S2-11 真实库/初始化重跑/索引计划、第 10 章上线准入、A 矩阵业务验收。\n";
    md
}

// ---- args ----
let wave = "";
let requested_ids = [];
let land_arg = ();
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
}
if !is_text(wave) {
    pause("verification", "Pass args.wave（如 s2-consumers）。inventory 只输出编目。");
}
if !contains_text(known_waves(), wave) {
    pause("verification", "Unknown args.wave. Use one of the documented wave names.");
}

let all_batches = catalog();
phase("Select");
if wave == "inventory" {
    let path = write_scratch_file("remaining-batches.md", inventory_report(all_batches));
    complete(#{ path: path, wave: "inventory", next_wave: "s2-consumers" });
}

let selected = [];
for batch in all_batches {
    if batch.wave == wave {
        if requested_ids.len() == 0 || contains_text(requested_ids, batch.id) {
            selected.push(batch);
        }
    }
}
if selected.len() == 0 {
    pause("verification", "No batches matched args.wave/batch_ids.");
}
if selected.len() > 3 {
    pause("verification", "At most 3 batches per run. Pass args.batch_ids to subset.");
}

let land = selected.len() == 1;
if land_arg != () {
    land = land_arg;
}

log("wave=" + wave + " batches=" + json_encode(selected.len()) + " land=" + json_encode(land));

// ---- implement ----
phase("Implement");
let impl_results = [];
if land {
    for batch in selected {
        let r = agent(implement_prompt(batch, true), #{
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
            prompt: implement_prompt(batch, false),
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
            prompt: review_prompt(batch),
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

// ---- report ----
phase("Report");
let md = "";
md += "# 组织数据范围实施报告\n\n";
md += "- wave: ";
md += wave;
md += "\n- land: ";
md += json_encode(land);
md += "\n- next_wave: ";
md += next_wave_name(wave);
md += "\n\n| 批次 | 实现 | 评审 | 验证 | 摘要 |\n| --- | --- | --- | --- | --- |\n";

i = 0;
let completed_ids = [];
let blocked_ids = [];
for batch in selected {
    let impl_state = "failed";
    let review_state = "skipped";
    let verify_state = "skipped";
    let summary = "";
    let r = impl_results[i];
    if impl_ok[i] {
        impl_state = "ok";
        completed_ids.push(batch.id);
        if is_map(r.output) && is_text(r.output.summary) {
            summary = r.output.summary;
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
    md += summary;
    md += " |\n";
    i += 1;
}

md += "\n未纳入本 workflow 的强制人工项：S2-11、第 10 章上线准入、A01-A32 真实账号与数据库验收。本地检查通过不得登记已验收。\n";
let report_path = write_scratch_file("org-data-scope-wave-report.md", md);
complete(#{
    path: report_path,
    wave: wave,
    land: land,
    completed: completed_ids,
    blocked: blocked_ids,
    next_wave: next_wave_name(wave),
});
