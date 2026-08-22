let meta = #{
    name: "erp-e2e-flows",
    description: "基于 docs/erp-phase-1.md 业务流程编写 Playwright E2E 测试：每个流程一个并行 agent，先探查前后端实际实现，再编写 spec，并汇报文档-代码差异与风险",
    phases: [
        #{ title: "探查并编写流程测试", detail: "每个流程一个 agent：读代码 → 写 spec → 汇报差异" },
    ],
};

// ---- 流程清单（每个元素对应一个 spec 文件 = 一个业务流程）----
let flows = [
    #{ id: "flow-01", spec: "flow-01-sales-warehouse.spec.ts",
       title: "实物销售单仓发完整基准流程",
       doc: "erp-phase-1.md §7.3.1（商品入仓后由公司仓库发货）+ §9.1（客户票款）",
       focus: "客户创建(W03)→合同上传PDF(W04, fixtures/sample-contract.pdf)→销售单创建(W05,选sellable SKU)→提交→采购确认审批节点通过→销售单生效→采购建采购单(W08,选源/成本/交期)→财务审批通过→采购入库单(入仓)→仓发出库单→销售登记客户验收(W06)→财务登记客户回款单(W11)+销项发票并核销→断言销售单履约/回款/开票进度与关闭条件" },
    #{ id: "flow-02", spec: "flow-02-supplier-direct-ship.spec.ts",
       title: "供应商直接发客户（代发）",
       doc: "erp-phase-1.md §7.3.2",
       focus: "前段与 flow-01 相同（客户/合同/销售单→采购确认→生效→采购单→财务审批）；履约段替换为采购登记代发出库单（不经过仓库，不影响自有库存）→销售验收→断言库存台账无变化" },
    #{ id: "flow-03", spec: "flow-03-electronic-delivery.spec.ts",
       title: "虚拟商品电子交付",
       doc: "erp-phase-1.md §7.3.3",
       focus: "客户/合同→含虚拟商品明细的销售单→采购确认→生效→采购单(虚拟商品)→采购登记电子交付记录（对象/数量/时间/结果/凭证）→销售验收" },
    #{ id: "flow-04", spec: "flow-04-service-fulfillment.spec.ts",
       title: "线下服务履约",
       doc: "erp-phase-1.md §7.3.3",
       focus: "客户/合同→线下服务明细销售单→采购确认→生效→采购单(线下服务)→采购登记服务履约记录（对象/时间/地点/结果/凭证）→销售验收" },
    #{ id: "flow-05", spec: "flow-05-approval-rejection.spec.ts",
       title: "审批驳回与三条出路",
       doc: "erp-phase-1.md §4.4（驳回后：照原条件承接 / 撤回改单重提 / 作废）",
       focus: "销售建单提交→采购在采购确认节点驳回并填原因→断言销售单不生效、内容不变、审批轮次加一并回到首节点→场景A:照原条件承接重新走审批最终生效；场景B:驳回后销售作废销售单" },
    #{ id: "flow-06", spec: "flow-06-customer-funds.spec.ts",
       title: "客户票款：回款、销项发票、核销与冲正",
       doc: "erp-phase-1.md §9.1 + §6.5.4（资金与发票纠错）",
       focus: "准备一张已生效销售单（最快UI路径）→财务创建客户回款单草稿→提交→销售领导(lisiyong)审批→过账→核销应收→断言应收结清；销项发票登记与核销；回款冲正单→提交→审批→过账→断言原回款冲正" },
    #{ id: "flow-07", spec: "flow-07-supplier-funds.spec.ts",
       title: "供应商票款：付款、进项发票、核销与冲正",
       doc: "erp-phase-1.md §9.2 + §6.5.4",
       focus: "准备一张已生效采购单→财务创建供应商付款单草稿→提交→采购(caigou)审批→过账→核销应付；进项发票登记；付款冲正单→提交→审批→过账" },
    #{ id: "flow-08", spec: "flow-08-sales-change-order.spec.ts",
       title: "销售变更单",
       doc: "erp-phase-1.md §6.5.1",
       focus: "销售单生效（不履约）→销售创建销售变更单（调整数量/金额并填原因）→提交→采购确认节点→销售领导复核节点→生效→断言销售单金额/明细按变更更新" },
    #{ id: "flow-09", spec: "flow-09-purchase-change-order.spec.ts",
       title: "采购变更单",
       doc: "erp-phase-1.md §6.5.2",
       focus: "采购单生效→采购创建采购变更单（调整数量/成本并填原因）→提交→财务复核节点→生效→断言采购单/应付按变更更新" },
    #{ id: "flow-10", spec: "flow-10-stock-adjustment.spec.ts",
       title: "库存调整",
       doc: "erp-phase-1.md §6.5.5",
       focus: "以 admin 创建库存调整单（盘亏/盘盈）→提交→财务(caiwu)审批→过账→断言库存台账库存数量变化；注意无仓储账号，实际权限以代码为准" },
];

let report_schema = #{
    "type": "object",
    "required": ["flow", "spec_file", "written", "doc_mismatches", "risks"],
    "properties": #{
        "flow": #{ "type": "string" },
        "spec_file": #{ "type": "string" },
        "written": #{ "type": "boolean" },
        "doc_mismatches": #{
            "type": "array",
            "items": #{
                "type": "object",
                "required": ["doc", "code"],
                "properties": #{
                    "doc": #{ "type": "string" },
                    "code": #{ "type": "string" }
                }
            }
        },
        "risks": #{
            "type": "array",
            "items": #{ "type": "string" }
        },
        "notes": #{ "type": "string" }
    }
};

// 过滤：args.only = "flow-03" 只跑单流程（调试用）
let selected = [];
if args != () && args.only != () {
    for f in flows {
        if f.id == args.only {
            selected.push(f);
        }
    }
    if selected.len() == 0 {
        pause("verification", "args.only 未匹配任何流程 id；可用值: flow-01 .. flow-10");
    }
} else {
    selected = flows;
}

phase("探查并编写流程测试");

let jobs = [];
let i = 0;
for f in selected {
    i += 1;
    let prompt = "你是资深测试工程师，为 ERP（员工福利）系统编写 Playwright E2E 测试。\n";
    prompt += "流程: [" + f.id + "] " + f.title + "\n";
    prompt += "文档依据: docs/erp-phase-1.md " + f.doc + "\n";
    prompt += "流程要点: " + f.focus + "\n\n";
    prompt += "## 必读（动手前必须用 read_file/grep 实际阅读，禁止凭记忆猜测选择器）\n";
    prompt += "1. e2e/README.md、e2e/playwright.config.ts、e2e/helpers/ 下全部文件（accounts.ts / login.ts / api.ts / ui.ts）——必须复用这些工具；\n";
    prompt += "2. docs/erp-phase-1.md 对应章节（用 read_file 按行号读取相关段落）；\n";
    prompt += "3. 前端实际实现（以代码为准！）：erp-client/app/(workspace)/ 下相关路由页面、erp-client/features/ 下相关 feature 的 pages/components/api/queries（读组件源码确定按钮文案、输入框 placeholder/label、表格列、弹窗标题、toast 文案、路由与查询参数），例如 sales-orders / customers / contracts / purchase-orders / fulfillment-operations / inventory / customer-receivables / supplier-payables / sales-orders(acceptance) 等 feature 目录；\n";
    prompt += "4. 必要时读 backend/services 下对应服务的 HTTP 路由（backend/apps/web-api/src/core/routes/）确认接口路径与字段，辅助理解页面行为。\n\n";
    prompt += "## 环境事实（已确认）\n";
    prompt += "- 前后端已在运行：前端 http://localhost:3000（Next.js SPA），后端 http://127.0.0.1:10001；\n";
    prompt += "- 账号（密码全部 123456）：xiaoshou=销售、caigou=采购、yunying=运营、caiwu=财务、lisiyong=销售领导、admin=超级管理员；\n";
    prompt += "- 已发布的审批定义（提交人不得审批自己的单据）：SalesOrder=采购确认(caigou)；VoucherSalesOrder=销售总监审批(lisiyong)→运营审批(yunying)→财务审批(caiwu)；SalesChangeOrder=采购确认(caigou)→销售领导复核(lisiyong)；PurchaseOrder=财务审核(caiwu)；PurchaseChangeOrder=财务复核(caiwu)；StockAdjustment=财务审批(caiwu)；CustomerReceipt=销售领导复核(lisiyong)；SupplierPayment=采购复核(caigou)；CustomerRefund=销售领导复核(lisiyong)；SupplierRefund=采购复核(caigou)；ReceiptReversal=销售领导复核(lisiyong)；PaymentReversal=采购复核(caigou)；\n";
    prompt += "- 待办审批在 W02 统一待办（/workspace/tasks）或对应工作面的审批区完成；\n";
    prompt += "- 数据库在每个流程前已重置（业务数据为空、无种子），所有业务数据必须通过 UI 创建；账号/RBAC 与供应商/商品/仓库主数据保留；\n";
    prompt += "- 合同上传需要 PDF：e2e/fixtures/sample-contract.pdf（使用绝对路径或相对 e2e 目录路径上传）。\n\n";
    prompt += "## 编写要求\n";
    prompt += "1. 输出文件：e2e/tests/" + f.spec + "（必须真实写入，用 write 工具）；\n";
    prompt += "2. 用 TypeScript 编写单个 test（可拆多个 test 但需串行且共享流程状态，建议一个 test 覆盖全流程，分步骤注释）；\n";
    prompt += "3. 账号切换用 helpers/login.ts 的 loginViaUi / newLoggedInContext（每个账号独立 context）；\n";
    prompt += "4. 选择器必须来自实际读到的组件代码：优先 getByRole/getByLabel/getByPlaceholder/getByText 与 data-testid；禁止使用脆弱的 nth() 链式索引；\n";
    prompt += "5. 每个关键里程碑都要断言（弹窗关闭/列表出现新行/状态徽标变化/toast），用 expect(...).toBeVisible({timeout:20000}) 等；\n";
    prompt += "6. 不要调用 run-flow.sh / playwright 命令，不要修改 e2e/helpers 与其他 spec 文件，流程内专用小工具函数写在 spec 文件顶部；\n";
    prompt += "7. 文档与代码不一致时以代码为准，并在 doc_mismatches 中逐条登记（doc=文档说法，code=代码实际行为）；把可能影响测试稳定性的点写入 risks；\n";
    prompt += "8. spec 头部用注释写明：流程名、文档章节、使用账号、发现的文档-代码差异；\n";
    prompt += "9. 文件必须语法正确（TS/ESM 风格与现有 spec 一致：import { test, expect } from '@playwright/test'，相对导入 ../helpers/xxx）。\n\n";
    prompt += "完成标准：读完上述代码、真实写入 spec 文件后，返回结构化报告。若某些 UI 细节在代码中找不到（例如按钮文案不确定），在 risks 中列出并选择最可能的实现继续写，不得编造不存在的选择器。";

    jobs.push(#{
        prompt: prompt,
        label: f.id + ":" + f.title,
        capability_mode: "read-write",
        output_schema: report_schema,
    });
}

let results = parallel(jobs);

let created = [];
let mismatches = [];
let risks = [];
let idx = 0;
for r in results {
    if r != () && r.success && r.output != () {
        if r.output.written == true {
            created.push(r.output.spec_file);
        }
        if r.output.doc_mismatches != () {
            for m in r.output.doc_mismatches {
                mismatches.push(r.output.spec_file + " | 文档: " + m.doc + " => 代码: " + m.code);
            }
        }
        if r.output.risks != () {
            for k in r.output.risks {
                risks.push(r.output.spec_file + " | " + k);
            }
        }
    } else {
        risks.push(selected[idx].id + " | agent 失败或无报告");
    }
    idx += 1;
}

log("已生成 spec: " + created.len().to_string() + " 个");
log("文档-代码差异: " + mismatches.len().to_string() + " 条");
log("风险: " + risks.len().to_string() + " 条");

complete(#{
    summary: "已生成 " + created.len().to_string() + " 个流程 spec",
    created: created,
    doc_mismatches: mismatches,
    risks: risks,
});
