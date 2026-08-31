let meta = #{
    name: "erp-e2e-flows",
    description: "基于 docs/erp-phase-1.md 正常履约与异常处理流程编写 Playwright E2E 测试：每个流程一个并行 agent，先探查前后端实际实现，再编写 spec，并汇报文档-代码差异与风险",
    phases: [
        #{ title: "探查并编写流程测试", detail: "每个流程一个 agent：读代码 → 写 spec → 汇报差异" },
    ],
};

// ---- 流程清单（每个元素对应一个 spec 文件 = 一个业务流程）----
// 01-04 外部采购履约四条路线；05 销售审批驳回；06-07 票款；08-10 变更/库存调整；
// 11-13 库存直配、拆分供给、先款后货；14-19 采购驳回与异常纠正；20 卡券票款复核。
let flows = [
    #{ id: "flow-01", spec: "flow-01-sales-warehouse.spec.ts",
       title: "外部采购入仓后仓发完整基准流程",
       doc: "erp-phase-1.md §7.3.1 + §7.4（供给分配）+ §9.1/§9.3（票款与关闭）",
       focus: "客户创建(W03)→合同上传PDF(W04, fixtures/sample-contract.pdf)→销售单创建(W05,选具备销售资格的实物SKU,付款条件用账期或货到付款)→提交→采购在W01采购确认节点只通过/驳回（不选源、不录入成本/交期）→全部节点通过后销售单生效并生成供给分配任务(PROCUREMENT_ORDER_CREATION)→采购在W01确认供给分配：全部走外部采购入仓（销售单/审批不得保存供应商或履约路线）→系统创建采购单并立即提交审批（不得留未提交草稿）→财务总监(caiwu)审批通过后采购单生效并形成应付→本流程先履约后付款：仓储(cangchu)登记采购入库单→仓发出库单→销售登记客户验收通过(W06/W01验收任务)→出纳(fukuan)登记客户回款并提交、caiwu审批入账；开票人(kaipiao)登记销项发票并核销→断言：验收后未回款不得关闭；应收结清后关闭；开票未完成不阻塞关闭。分别断言履约/回款/开票进度。" },
    #{ id: "flow-02", spec: "flow-02-supplier-direct-ship.spec.ts",
       title: "供应商直接发客户（代发）",
       doc: "erp-phase-1.md §7.3.2 + §7.4",
       focus: "前段与 flow-01 相同到供给分配；供给分配确认履约责任为供应商直发/代发→采购单财务审批生效→采购在W01履约任务登记代发出库单（不经过仓库）→销售验收→断言自有库存台账无变化、无采购入库单。" },
    #{ id: "flow-03", spec: "flow-03-electronic-delivery.spec.ts",
       title: "虚拟商品电子交付",
       doc: "erp-phase-1.md §7.3.3 + §7.4",
       focus: "客户/合同→含虚拟商品明细的销售单→采购确认通过→生效→供给分配只能推荐采购、不得分配现有库存→采购单生效→采购登记电子交付记录（对象/数量/时间/结果/凭证）→销售验收。" },
    #{ id: "flow-04", spec: "flow-04-service-fulfillment.spec.ts",
       title: "线下服务履约",
       doc: "erp-phase-1.md §7.3.3 + §7.4",
       focus: "客户/合同→线下服务明细销售单→采购确认通过→生效→供给分配只能推荐采购、不得分配现有库存→采购单生效→采购登记服务履约记录（对象/时间/地点/结果/凭证）→销售验收。" },
    #{ id: "flow-05", spec: "flow-05-approval-rejection.spec.ts",
       title: "销售单审批驳回与三条出路",
       doc: "erp-phase-1.md §4.4（照原条件承接 / 撤回改单重提 / 作废）",
       focus: "销售建单提交→采购在采购确认节点驳回并填原因→断言销售单不生效、内容与subject_version不变、审批轮次加一并回到首节点；此时禁止创建销售变更单。场景A 照原条件承接：不撤回不改单，新一轮各节点通过后生效。场景B 撤回改单重提：撤回使销售单回到草稿→修改数量或金额→再次提交→审批通过后生效。场景C 不做：驳回后销售作废销售单，主状态已作废。" },
    #{ id: "flow-06", spec: "flow-06-customer-funds.spec.ts",
       title: "客户票款：分次回款、销项发票、核销与冲正",
       doc: "erp-phase-1.md §9.1 + §6.5.4（回款冲正）+ §9.3（关闭与开票）",
       focus: "最快UI路径准备一张已生效实物销售单（可未履约；回款允许在履约前登记）→出纳(fukuan)分两次创建客户回款草稿并提交→财务总监(caiwu)审批入账并核销→断言先部分回款再结清；禁止caiwu自己提交回款（岗位分离）。若UI支持一张回款核销多张销售单则覆盖，否则记doc_mismatches。开票人(kaipiao)登记销项发票并核销；开票完成不是关闭条件。回款冲正：fukuan提交→lisiyong确认依据→caiwu审批入账→断言原回款已冲正、应收重新打开。客户退款见flow-16，红票见flow-17。" },
    #{ id: "flow-07", spec: "flow-07-supplier-funds.spec.ts",
       title: "供应商票款：出纳付款任务、进项发票、核销与冲正",
       doc: "erp-phase-1.md §9.2 + approval-workflow-contract.md §4.3（SupplierPayment=NO_APPROVAL）+ §6.5.4",
       focus: "准备已生效采购单→断言形成应付且W01出现出纳付款任务(SUPPLIER_PAYMENT_EXECUTION)，不得出现SupplierPayment审批实例或审批任务→出纳(fukuan)在付款任务核对收款账户（户名/开户行/脱敏账号）后确认付款；允许分两次，第二次仍在同一开放任务上→断言付款单直接确认入账、核销应付、任务在应付清零后完成。进项发票登记。付款冲正：fukuan提交→caigou确认依据→caiwu审批入账。禁止再走创建付款草稿再送审的旧路径；付款页面不得出现提交审批入口；禁止caiwu自己提交冲正。" },
    #{ id: "flow-08", spec: "flow-08-sales-change-order.spec.ts",
       title: "销售变更单（未履约）",
       doc: "erp-phase-1.md §6.5.1 + §4.4（生效后才允许变更单）",
       focus: "销售单生效且未履约、未产生出入库→销售创建销售变更单（调整数量/金额并填原因）→提交→caigou采购确认履约影响→caiwu财务复核金额与应收→生效→断言销售单金额/明细/应收按变更更新。负向：对审批中或驳回未生效销售单不得创建变更单。已发生回款或出入库时不得直接改原单（本spec只覆盖未执行变更）。" },
    #{ id: "flow-09", spec: "flow-09-purchase-change-order.spec.ts",
       title: "采购变更单（未执行）",
       doc: "erp-phase-1.md §6.5.2",
       focus: "采购单生效且未入库、未付款→采购创建采购变更单（调整数量/成本并填原因）→提交→cangchu仓储确认库存发货影响→caiwu财务复核金额与应付→生效→断言采购单/应付按变更更新。已发生付款或出入库时不得直接改原单，须通过纠正单（本spec不走已执行纠正）。" },
    #{ id: "flow-10", spec: "flow-10-stock-adjustment.spec.ts",
       title: "库存调整：盘盈、盘亏、损坏与驳回",
       doc: "erp-phase-1.md §6.5.5",
       focus: "仓储(cangchu)创建库存调整单：分别覆盖盘盈、盘亏，有损坏入口则覆盖损坏→提交→caiwu审批通过→确认入账→断言库存数量变化并产生正式流水。禁止caiwu自己提交。另测一次驳回：驳回后轮次加一回到首节点，库存不变。原出入库记录保留。" },
    #{ id: "flow-11", spec: "flow-11-inventory-warehouse.spec.ts",
       title: "现有库存仓发（零采购单）",
       doc: "erp-phase-1.md §7.2/§7.4（现有库存直配跳过采购/付款/入库）",
       focus: "先由cangchu用库存调整盘盈准备指定仓库+SKU的可用库存（caiwu审批）→客户/合同/销售单（数量不超过可用库存）→采购确认通过→生效→供给分配确认全部走现有库存→断言：零张采购单、available减少且reserved增加、形成仓发草稿→cangchu仓发出库消耗预占→销售验收。不得出现采购单、供应商付款、采购入库。" },
    #{ id: "flow-12", spec: "flow-12-split-allocation.spec.ts",
       title: "同一销售明细拆分：库存直配 + 采购缺口",
       doc: "erp-phase-1.md §7.4（同一明细允许库存与采购拆分满足）",
       focus: "先由cangchu盘盈准备少于销售数量的库存（caiwu审批）→销售单数量大于库存→供给分配：部分现有库存 + 剩余采购缺口（同一明细拆分，合计不得超过需求）→断言同时产生库存预占、仓发草稿和采购单，且不得超分配→采购单审批→cangchu入库后沿采购销售分配预占→仓发两段履约数量合计等于销售明细→验收。" },
    #{ id: "flow-13", spec: "flow-13-payment-before-fulfillment.spec.ts",
       title: "先款后货：付款完成前不得履约",
       doc: "erp-phase-1.md §7.2/§7.4（先款后货时付款完成后发货）",
       focus: "销售单生效→供给分配创建采购单时付款条件为先款后货→采购单审批生效并形成付款任务→断言付款完成前入库/代发/电子交付/服务履约入口不可用或提交失败→出纳(fukuan)完成付款任务后才能登记履约。与flow-01（账期/货到付款先履约）对照。" },
    #{ id: "flow-14", spec: "flow-14-purchase-order-rejection.spec.ts",
       title: "采购单审批驳回",
       doc: "erp-phase-1.md §7.1/§7.2（采购审批驳回：轮次加一回到首节点）",
       focus: "销售单生效→供给分配创建采购单并提交审批→财务在采购单审批节点驳回并填原因→断言采购单不生效、不形成应付、内容不变、轮次加一回到首节点→不改单由财务再通过→采购单生效并形成应付。禁止用采购变更单处理尚未生效的驳回单。" },
    #{ id: "flow-15", spec: "flow-15-sales-return.spec.ts",
       title: "客户拒收/退货全链（含采购退货、退款与红票）",
       doc: "erp-phase-1.md §6.2（验收短少/拒收）+ §6.5.3（退货与拒收时序）",
       focus: "走最短仓发或代发至待验收→销售登记验收短少或拒收→创建销售退货/拒收处理单→仓储(cangchu)验收退回商品；直退供应商时由采购登记退回结果、不经公司仓库→采购创建采购退货单并出库或直退→出纳(fukuan)提交客户退款/供应商退款（caiwu审批），开票人(kaipiao)登记销项/进项红票→销售确认处理完成。原出库/回款/发票事实保留，通过反向记录纠正。若代码仅有草稿而无提交/过账命令，按能走到的步骤写并在doc_mismatches登记缺口。" },
    #{ id: "flow-16", spec: "flow-16-customer-refund.spec.ts",
       title: "客户退款单",
       doc: "erp-phase-1.md §6.3/§6.5.4（客户退款单，PROCESS_REQUIRED）",
       focus: "在已确认入账的客户回款基础上，出纳(fukuan)创建客户退款单（与回款冲正不同）并提交→lisiyong确认退款依据→caiwu审批入账→断言应收恢复、原回款记录保留。禁止caiwu自己提交。另测一次驳回：轮次加一回到首节点，退款未入账。供应商退款：fukuan提交→caigou确认依据→caiwu审批。" },
    #{ id: "flow-17", spec: "flow-17-red-invoice.spec.ts",
       title: "发票红冲（销项与进项，无审批）",
       doc: "erp-phase-1.md §6.5.4（红票=Invoice，NO_APPROVAL）",
       focus: "已登记销项发票→开票人(kaipiao)按Invoice强类型命令登记销项红票并关联原发票→断言无审批实例/审批任务、原发票保留、可开票金额回退。已登记进项发票则同样登记进项红票。禁止把红票送进审批流。" },
    #{ id: "flow-18", spec: "flow-18-supply-invalid.spec.ts",
       title: "供给失效不得建采购单，必须走销售变更",
       doc: "erp-phase-1.md §7.4（供给失效/停止可供/资质失效时采购单不得创建，走销售变更单）+ §4.4（生效后禁止回到审批）",
       focus: "销售单生效后、供给分配确认前，采购停用该SKU的有效供给或使资质失效→断言供给分配不得创建采购单、不得虚增库存或供应商可供量→必须通过销售变更单改品或改量后再分配。禁止回到销售审批改选源。" },
    #{ id: "flow-19", spec: "flow-19-void-release-reservation.spec.ts",
       title: "生效后作废释放库存预占",
       doc: "erp-phase-1.md §7.4（预占只能通过变更、作废、采购退货或库存调整释放）+ §10.2",
       focus: "按flow-11建立现有库存预占（仓发出库前）→销售作废已生效销售单→断言预占释放、available恢复、仓发草稿关闭或作废、不得再履约。已出库事实不回退，本spec停在出库前。" },
    #{ id: "flow-20", spec: "flow-20-card-funds-review.spec.ts",
       title: "卡券销售单票款复核",
       doc: "erp-phase-1.md §8.7 + §9.1（同步金额变化后应收重算）+ W13",
       focus: "探查W13卡券票款复核与W17/W18如何形成已映射卡券销售单。若存在导入/同步/管理入口：映射完成前不得派生应收；形成CARD_FUNDS_REVIEW期初复核任务后财务总监(caiwu)完成复核，复核完成前应收台账必须显示待复核；再由出纳(fukuan)登记回款、开票人(kaipiao)登记销项发票。若能触发同步金额变化，覆盖CARD_FUNDS_DELTA_REVIEW：应收按最新版本重算、已登记回款和发票不受影响、差额进入待复核。若一期无法从UI创建卡券销售单，spec写明前置条件与跳过策略并登记risks，不得伪造商城或手工补建另一张销售单。" },
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
let available_ids = "";
for f in flows {
    if available_ids != "" {
        available_ids += ", ";
    }
    available_ids += f.id;
}
if args != () && args.only != () {
    for f in flows {
        if f.id == args.only {
            selected.push(f);
        }
    }
    if selected.len() == 0 {
        pause("verification", "args.only 未匹配任何流程 id；可用值: " + available_ids);
    }
} else {
    selected = flows;
}

phase("探查并编写流程测试");

let jobs = [];
for f in selected {
    let prompt = "你是资深测试工程师，为 ERP（员工福利）系统编写 Playwright E2E 测试。\n";
    prompt += "流程: [" + f.id + "] " + f.title + "\n";
    prompt += "文档依据: docs/erp-phase-1.md " + f.doc + "\n";
    prompt += "流程要点: " + f.focus + "\n\n";
    prompt += "## 必读（动手前必须用 read_file/grep 实际阅读，禁止凭记忆猜测选择器）\n";
    prompt += "1. 仓库根 playwright.config.ts、helpers/ 下全部文件（accounts.ts / login.ts / api.ts / ui.ts）——必须复用这些工具。编排脚本在 scripts/（run-flow.sh / reset-db.sh），禁止新建或写入 e2e/ 目录；\n";
    prompt += "2. docs/erp-phase-1.md 对应章节（用 read_file 按行号读取相关段落）；涉及审批政策时再读 docs/approval-workflow-contract.md §4.3/§4.4 与 docs/workbench-workitem-contract.md 第3节；\n";
    prompt += "3. 前端实际实现（以代码为准！）：erp-client/app/(workspace)/ 下相关路由页面、erp-client/features/ 下相关 feature 的 pages/components/api/queries（读组件源码确定按钮文案、输入框 placeholder/label、表格列、弹窗标题、toast 文案、路由与查询参数）。重点目录包括 workspace（W01 工作台原地处理）、sales-orders、customers、contracts、purchase-orders、fulfillment-operations、inventory、customer-receivables、supplier-payables、card-funds-review、master-data；\n";
    prompt += "4. 必要时读 backend/services 下对应服务的 HTTP 路由（backend/apps/web-api/src/core/routes/）确认接口路径与字段，辅助理解页面行为。\n\n";
    prompt += "## 环境事实（已确认）\n";
    prompt += "- 前后端已在运行：前端 http://localhost:3000（Next.js SPA），后端 http://127.0.0.1:10001；\n";
    prompt += "- 账号（密码全部 123456）：xiaoshou=销售、lisiyong=销售领导、caigou=采购、yunying=运营、cangchu=仓储、caiwu=财务总监、fukuan=出纳、kaipiao=开票人、admin=超级管理员；\n";
    prompt += "- 已发布的审批定义（提交人不得出现在任何审批节点）：SalesOrder=采购确认(caigou)，xiaoshou提交；VoucherSalesOrder=销售领导(lisiyong)→运营(yunying)→财务总监(caiwu)；SalesChangeOrder=采购确认(caigou)→财务复核(caiwu)；PurchaseOrder=财务总监(caiwu)，caigou提交；PurchaseChangeOrder=仓储确认(cangchu)→财务复核(caiwu)；StockAdjustment=财务审批(caiwu)，cangchu提交；CustomerReceipt=财务总监(caiwu)，必须fukuan提交；CustomerRefund=销售领导(lisiyong)→财务总监(caiwu)，fukuan提交；SupplierRefund=采购(caigou)→财务总监(caiwu)，fukuan提交；ReceiptReversal=销售领导(lisiyong)→财务总监(caiwu)，fukuan提交；PaymentReversal=采购(caigou)→财务总监(caiwu)，fukuan提交。禁止caiwu自己提交回款/退款/冲正；\n";
    prompt += "- NO_APPROVAL（不得创建审批实例或审批任务）：SupplierPayment、PurchaseReceipt、Delivery、ElectronicDelivery、ServiceFulfillment、CustomerAcceptance、Invoice、SalesReturnCase、PurchaseReturnOrder。供应商付款由出纳fukuan在W01付款任务(SUPPLIER_PAYMENT_EXECUTION)直接确认入账，采购单最终通过即付款授权，禁止再启动付款审批；红票由kaipiao走Invoice强类型命令，禁止送审；\n";
    prompt += "- 待办只在 W01 我的工作台（/workspace）原地处理：审批=DOCUMENT_APPROVAL；供给分配=PROCUREMENT_ORDER_CREATION；履约=FULFILLMENT_OPERATION；验收=CUSTOMER_ACCEPTANCE_REGISTRATION；供应商付款=SUPPLIER_PAYMENT_EXECUTION；销项开票=SALES_INVOICE_EXECUTION；卡券票款复核=CARD_FUNDS_REVIEW / CARD_FUNDS_DELTA_REVIEW。/workspace/tasks 只重定向到 /workspace，禁止当作第二待办入口；\n";
    prompt += "- 供给分配是唯一选源环节：销售单生效后才生成供给分配任务；采购确认节点不选供给、不录入成本/交期。采购单由供给分配确认事务创建并立即提交审批，不得留下未提交草稿。纯库存覆盖必须零张采购单；\n";
    prompt += "- 界面文案以代码和 ui-glossary 为准，资金/库存确认入账不要用过账一词去匹配按钮；\n";
    prompt += "- 数据库在每个流程前已重置（业务数据为空、无种子），所有业务数据必须通过 UI 创建；账号/RBAC 与供应商/商品/仓库主数据保留。现有库存场景必须先用库存调整盘盈准备可用量，不要假设期初库存；\n";
    prompt += "- 合同上传需要 PDF：fixtures/sample-contract.pdf（仓库根 fixtures/，使用绝对路径或相对仓库根路径上传）。\n\n";
    prompt += "## 编写要求\n";
    prompt += "1. 输出文件：tests/" + f.spec + "（仓库根 tests/，必须真实写入，用 write 工具）；\n";
    prompt += "2. 用 TypeScript 编写单个 test（可拆多个 test 但需串行且共享流程状态，建议一个 test 覆盖全流程，分步骤注释）；负向断言（不得建采购单、不得履约、不得关闭、不得开变更单、不得出现审批实例）必须写成明确的 expect；\n";
    prompt += "3. 账号切换用 helpers/login.ts 的 loginViaUi / newLoggedInContext（每个账号独立 context）；\n";
    prompt += "4. 选择器必须来自实际读到的组件代码：优先 getByRole/getByLabel/getByPlaceholder/getByText 与 data-testid；禁止使用脆弱的 nth() 链式索引；\n";
    prompt += "5. 每个关键里程碑都要断言（弹窗关闭/列表出现新行/状态徽标变化/toast），用 expect(...).toBeVisible({timeout:20000}) 等；\n";
    prompt += "6. 不要调用 scripts/run-flow.sh / playwright 命令，不要修改 helpers/、scripts/ 与其他 spec 文件，流程内专用小工具函数写在 spec 文件顶部；\n";
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
