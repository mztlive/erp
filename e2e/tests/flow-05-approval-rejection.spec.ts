/**
 * [flow-05] 审批驳回与三条出路
 *
 * 文档依据: docs/erp-phase-1.md §4.4（任一节点驳回：销售单不生效、内容不变、
 *   审批轮次加一并回到第一个节点重新审批；销售只有三条出路：改品或改价（撤回→改→重提）/
 *   照原条件承接（不撤回不改单，下一轮重新决定）/ 不做（作废销售单）），§10.2，
 *   以及 docs/ui-workspaces/w05-sales-orders.md §5.4。
 *
 * 使用账号: xiaoshou(销售) / caigou(采购)，密码 123456（每账号独立 context）。
 *
 * 流程（4 个串行 test 共享流程状态；刻意未启用 serial 模式——个别断言失败不阻断
 * 后续可验证步骤，见 risks）:
 *   01 销售建客户 → 上传合同 PDF → 建两张实物/服务销售单并提交（SalesOrder 定义：采购确认 1 节点）
 *   02 采购在 W01 工作台看到「采购确认」任务；经决策 API 驳回并填原因；
 *      断言销售单不生效、内容不变、审批轮次 +1 且回到首节点（审批实例 API 佐证）
 *   03 场景A（照原条件承接）: 不撤回不改单，对第 2 轮开放任务重新「通过」→ 销售单生效
 *   04 场景B（驳回后作废）: 第二张单被驳回后，销售撤回审批回草稿、再作废
 *
 * ⚠️ 当前构建的关键缺口（详见文末 doc_mismatches / risks，均已对照源码核实）:
 *   - 通过/驳回在 UI 无任何入口：工作项 allowed_actions 仅 VIEW/PROCESS/REASSIGN/CLOSE
 *     （backend/services/src/work_item/dto.rs WorkItemAllowedAction 枚举），单据层
 *     allowed_actions 仅 CANCEL/SUBMIT（services/src/sales_order/query.rs），审批区动作
 *     是两者并集 → APPROVE/REJECT 按钮不会渲染；
 *   - POST /admin/approval-decisions 当前为桩实现（services/src/approval/execution/
 *     runtime_service.rs submit_decision 丢弃 decision/reason/expected_task_version/
 *     idempotency_key，只回显视图，不推进 BPM 实例），crates/bpm/src/engine/decision.rs
 *     的 decide/apply_approve/apply_reject 没有任何调用方。
 *     → 测试 02/03 的「轮次 +1」「最终生效」断言在当前构建必然失败（用于暴露该缺口）；
 *     「撤回审批」（POST /admin/sales-orders/{id}/cancel-approval）与「作废」
 *     （POST /admin/sales-orders/{id}/void）接口已接线，测试 04 应通过。
 *
 * 发现的文档-代码差异:
 *   1. docs 称 W02 统一待办（/workspace/tasks）；代码中该路由 permanentRedirect 到
 *      /workspace（W01 唯一工作台），审批任务在 W01 右侧详情展示。
 *   2. docs 称任一节点可驳回并填原因；代码中 UI 无通过/驳回入口（见上），决策 API 为桩
 *      实现 → 本测试只能先经决策 API 提交决定并断言协议层回显（outcome/reason），
 *      轮次 +1 断言在当前构建必然失败。
 *   3. docs 称对象中心运行摘要展示轮次/节点/最近驳回；代码中销售单详情 approval.instance
 *      恒为 null、recent_history 为空（query.rs 传 None），工作台任务 DTO 也不下发轮次/
 *      驳回字段 → 轮次断言走审批实例 API（GET /admin/approval-instances?view=started）；
 *      且后端 /approval-instances/{id}/history 实际返回字段（instance_id/status/
 *      current_round_no/current_node_key/current_node_name）与前端历史 DTO
 *      （execution_id/round_no/result/node_key/decision_reason）不一致，历史 UI 无法渲染。
 *   4. docs 称出路一「撤回改单重提」在对象中心可用；代码中「撤回审批」按钮可见（CANCEL
 *      动作来自单据层）但 CancelApprovalDialog 仅在 approval.instance 存在时渲染（恒为
 *      null）→ UI 撤回不可用，只能走已接线的撤回接口。
 *   5. docs 称出路三「作废」在对象中心可用；代码中作废入口（canVoid/作废弹窗）挂在
 *      open_procurement_rejection 路径（resolve_open_procurement_rejection 恒返回 None），
 *      且 PendingReview→Voided 非法（实体 allowed_next 仅 [Draft, Effective]）→ 场景B
 *      先撤回至草稿，再调作废接口（Draft→Voided 已接线）。
 */
import path from "path"

import { expect, test } from "@playwright/test"
import type { APIRequestContext, Locator, Page } from "@playwright/test"

import { api, apiLogin } from "../helpers/api"
import { newLoggedInContext } from "../helpers/login"
import { gotoPage } from "../helpers/ui"

// ---------------------------------------------------------------------------
// 流程内专用小工具（不修改 helpers）
// ---------------------------------------------------------------------------

/** 生成唯一后缀，避免跨运行残留数据冲突（数据库每次 reset，正常不会重复）。 */
function uniqueSuffix(): string {
    return `${Date.now().toString().slice(-8)}${Math.floor(Math.random() * 90 + 10)}`
}

/** 未来 N 天的日期（本流程所有业务日期都取未来）。 */
function futureDate(days: number): Date {
    const d = new Date()
    d.setDate(d.getDate() + days)
    return d
}

const CONTRACT_PDF = path.join(__dirname, "..", "fixtures", "sample-contract.pdf")

/**
 * 通用 combobox（Base UI Combobox：input[role=combobox] + 弹层 [data-slot=combobox-item]）。
 * 支持本地过滤（枚举）与远程搜索（合同/客户/SKU/结算主体）。
 */
async function pickComboboxOption(
    page: Page,
    scope: Page | Locator,
    ariaLabel: string,
    optionText: string,
): Promise<void> {
    const input = scope.getByRole("combobox", { name: ariaLabel }).first()
    await input.click()
    await input.fill(optionText)
    const option = page
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: optionText })
        .first()
    await expect(option).toBeVisible({ timeout: 20_000 })
    await option.click()
}

/** 不输入搜索词，直接选弹层第一项（结算主体回退用）。 */
async function pickFirstComboboxOption(page: Page, ariaLabel: string): Promise<void> {
    const input = page.getByRole("combobox", { name: ariaLabel }).first()
    await input.click()
    const option = page.locator('[data-slot="combobox-item"]').first()
    await expect(option).toBeVisible({ timeout: 20_000 })
    await option.click()
}

/**
 * 日期选择：触发按钮（空态「选择日期」/ 已选「已选日期 YYYY-MM-DD」）→
 * react-day-picker 弹层。日格按钮带 data-day=toLocaleDateString(浏览器
 * locale)，兼容三种分隔格式；月份导航用 react-day-picker 默认英文按钮
 * （headless Chromium en-US）。
 */
async function pickDate(page: Page, label: string, date: Date): Promise<void> {
    const field = page
        .locator("label")
        .filter({ hasText: label })
        .first()
        .locator("xpath=..")
    const trigger = field
        .getByRole("button", { name: /^(选择日期|已选日期)/ })
        .first()
    const targetIso = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`
    // 已选日期与目标一致时直接跳过：点击同值不会关闭日历，且无需改动。
    const triggerLabel =
        (await trigger.getAttribute("aria-label")) ??
        ((await trigger.textContent()) ?? "")
    if (triggerLabel.includes(targetIso)) return
    await trigger.click()
    const calendar = page.locator('[data-slot="calendar"]').last()
    await expect(calendar).toBeVisible({ timeout: 10_000 })
    const targetCaption = new Intl.DateTimeFormat("en-US", {
        month: "long",
        year: "numeric",
    }).format(date)
    for (let i = 0; i < 24; i++) {
        const caption = calendar.locator(".rdp-caption_label").first()
        const text = await caption.textContent().catch(() => "")
        if (text?.includes(targetCaption)) break
        await calendar.getByRole("button", { name: "Next month" }).click()
    }
    const m = date.getMonth() + 1
    const d = date.getDate()
    const y = date.getFullYear()
    const padded = `${y}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`
    await calendar
        .locator(
            `[data-day="${m}/${d}/${y}"],[data-day="${y}/${m}/${d}"],[data-day="${padded}"]`,
        )
        .first()
        .click()
    await expect(calendar).not.toBeVisible({ timeout: 10_000 })
}

/** 打开工作台（W01）中某销售单的待办任务（任务按钮 id = work-item-{销售单id}）。 */
async function openWorkspaceTask(page: Page, orderId: string): Promise<void> {
    // 同一任务可能同时出现在工作台多个列表区，取第一个
    const task = page.locator(`#work-item-${orderId}`).first()
    await expect(task).toBeVisible({ timeout: 20_000 })
    await task.click()
}

// ---------------------------------------------------------------------------
// API 辅助（决策提交 / 审批实例轮次与历史佐证 / 撤回与作废的后端事实）
// ---------------------------------------------------------------------------

/** /admin/work-items 返回行（后端 WorkItemView 稳定字段）。 */
type WorkItemRow = {
    id: string
    work_item_type: string
    status: string
    business_object_id: string
    task_version?: string | number
}

/** POST /admin/approval-decisions 的响应视图。 */
type ApprovalCommandView = {
    instance_id: string
    instance_status: string
    current_round_no: number
    current_node_key?: string | null
    current_node_name?: string | null
    latest_rejection_reason?: string | null
    next_open_task?: {
        work_item_id: string
        task_version: number
        owner_user_id: string
    } | null
    outcome: string
}

/** /admin/approval-instances 列表行（含「我发起的」view）。 */
type ApprovalInstanceListItem = {
    instance_id: string
    status: string
    current_round_no: number
    current_node_key?: string | null
    current_node_name?: string | null
    document_id?: string | null
}

/** /admin/approval-instances/{id}/history 返回的执行行（后端实际形状）。 */
type ApprovalHistoryExecution = {
    instance_id: string
    status: string
    current_round_no: number
    current_node_key?: string | null
    current_node_name?: string | null
}

type BackendSalesOrderDetail = {
    id: string
    order_no: string
    version: number
    commercial_status: string
}

type SellableSkuRow = {
    sku_id: string
    sku_no: string
    product_kind: string
    name: string
}

type PartyRow = {
    id: string
    party_no: string
    status: string
}

/** 查找某人（采购）当前开放的销售单审批任务（scope=mine&family=approval）。 */
async function findOpenApprovalWorkItem(
    request: APIRequestContext,
    token: string,
    orderId: string,
): Promise<WorkItemRow> {
    const page = await api<{ items: WorkItemRow[] }>(
        request,
        "GET",
        "/admin/work-items",
        {
            token,
            query: { scope: "mine", family: "approval", page: 1, page_size: 50 },
        },
    )
    const row = page.items.find(
        (item) => item.business_object_id === orderId && item.status === "OPEN",
    )
    expect(row, `采购应有销售单 ${orderId} 的开放审批任务`).toBeTruthy()
    return row!
}

/**
 * 提交审批决定（通过/驳回）。
 * 当前构建 UI 无通过/驳回按钮（见头部注释），这是唯一可用的决定通道；
 * 当前实现为桩：请求成功并回显原因，但不推进审批实例（见 doc_mismatches/risks）。
 */
async function submitApprovalDecisionViaApi(
    request: APIRequestContext,
    token: string,
    orderId: string,
    decision: "APPROVE" | "REJECT",
    reason?: string,
): Promise<ApprovalCommandView> {
    const item = await findOpenApprovalWorkItem(request, token, orderId)
    const body: Record<string, string> = {
        work_item_id: item.id,
        decision,
        expected_task_version: String(item.task_version),
        idempotency_key: `flow05-${decision.toLowerCase()}-${orderId}-${Date.now()}`,
    }
    if (reason) body.reason = reason
    return api<ApprovalCommandView>(request, "POST", "/admin/approval-decisions", {
        token,
        body,
    })
}

/** 在「我发起的」审批列表中找到销售单的实例（销售视角，含轮次/节点/状态）。 */
async function findApprovalInstance(
    request: APIRequestContext,
    token: string,
    orderId: string,
): Promise<ApprovalInstanceListItem> {
    const page = await api<{ items: ApprovalInstanceListItem[] }>(
        request,
        "GET",
        "/admin/approval-instances",
        {
            token,
            query: { view: "started", document_type: "sales_order", limit: 50 },
        },
    )
    const row = page.items.find((item) => item.document_id === orderId)
    expect(row, `销售应在「我发起的」审批列表中看到销售单 ${orderId} 的实例`).toBeTruthy()
    return row!
}

// ---------------------------------------------------------------------------
// 测试数据（跨 test 共享；数据库每次流程前重置，前缀唯一避免误匹配）
// ---------------------------------------------------------------------------

const SUFFIX = uniqueSuffix()
const CUSTOMER_NAME = `E2E驳回客户${SUFFIX}`
const CREDIT_CODE = `91310000TEST${SUFFIX.slice(-6)}` // 18 位字母或数字
const CONTRACT_NO = `HT-E2E-${SUFFIX}`

/** 主数据（test 01 探测后共享）。 */
let skuNo = ""
let skuName = ""
let settlementParty: PartyRow | undefined

/** 流程状态（test 01 创建后共享给 02/03/04）。 */
let order1Id = ""
let order2Id = ""
let order1LinesBeforeReject = ""

test.describe("[flow-05] 审批驳回与三条出路", () => {
    test.setTimeout(300_000)

    test("01 前置：建客户、上传合同，建两张销售单并提交（审批中）", async ({
        browser,
        request,
    }) => {
        const salesToken = await apiLogin(request, "sales")

        // ---------- 主数据探测（可售 SKU / 结算主体，不随 reset 清除） ----------
        const skuPage = await api<{ items: SellableSkuRow[] }>(
            request,
            "GET",
            "/admin/sellable-skus",
            { token: salesToken, query: { page: 1, page_size: 20 } },
        )
        const sku = skuPage.items.find(
            (row) => row.product_kind.toUpperCase() !== "VOUCHER",
        )
        expect(sku, "数据库应保留至少一个可销售的实物/服务 SKU（主数据不随 reset 清除）").toBeTruthy()
        skuNo = sku!.sku_no
        skuName = sku!.name

        const partyPage = await api<{ items: PartyRow[] }>(
            request,
            "GET",
            "/admin/parties",
            { token: salesToken, query: { status: "active", page: 1, page_size: 10 } },
        )
        settlementParty = partyPage.items[0]

        // ---------- 销售登录，创建客户 ----------
        const { page: salesPage } = await newLoggedInContext(browser, "sales")
        await gotoPage(salesPage, "/sales/customers")
        // 页面操作区与空态各有一个「新建客户」，二者等价，取第一个
        await salesPage.getByRole("button", { name: "新建客户" }).first().click()
        const customerDialog = salesPage.getByRole("dialog").last()
        await expect(customerDialog).toBeVisible({ timeout: 20_000 })
        await customerDialog.getByLabel("法定名称").fill(CUSTOMER_NAME)
        await customerDialog.getByLabel("统一社会信用代码").fill(CREDIT_CODE)
        await pickComboboxOption(salesPage, customerDialog, "默认付款条件", "货到 30 天")
        await customerDialog.getByRole("button", { name: "创建客户" }).click()
        await expect(salesPage).toHaveURL(/\/sales\/customers\/[^/?]+/, {
            timeout: 20_000,
        })

        // ---------- 销售单 1（2 × 500 → 1,000.00，首次上传合同 PDF） ----------
        const order1 = await createAndSubmitSalesOrder({
            salesPage,
            contractNo: CONTRACT_NO,
            customerName: CUSTOMER_NAME,
            skuNo,
            skuName,
            settlementParty,
            quantity: "2",
            unitPrice: "500",
            expectedGross: /1,?000\.00/,
            uploadContract: true,
        })
        order1Id = order1.id
        // 提交后落在详情页：审批中、金额正确
        await expect(salesPage.getByText("审批中", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(salesPage.getByText(/1,?000\.00/).first()).toBeVisible()
        order1LinesBeforeReject = await salesPage
            .locator("table")
            .filter({ hasText: skuName })
            .first()
            .innerText()

        // ---------- 销售单 2（3 × 400 → 1,200.00，复用已上传合同） ----------
        const order2 = await createAndSubmitSalesOrder({
            salesPage,
            contractNo: CONTRACT_NO,
            customerName: CUSTOMER_NAME,
            skuNo,
            skuName,
            settlementParty,
            quantity: "3",
            unitPrice: "400",
            expectedGross: /1,?200\.00/,
            uploadContract: false,
        })
        order2Id = order2.id
        await expect(salesPage.getByText("审批中", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(salesPage.getByText(/1,?200\.00/).first()).toBeVisible()
    })

    test("02 采购驳回（决策 API）：销售单不生效、内容不变、轮次回到首节点加一", async ({
        browser,
        request,
    }) => {
        expect(order1Id, "前置 test 01 必须已创建销售单").toBeTruthy()
        const salesToken = await apiLogin(request, "sales")
        const procurementToken = await apiLogin(request, "procurement")

        // ---------- 采购在 W01 工作台看到「采购确认」任务 ----------
        const { page: procurementPage } = await newLoggedInContext(
            browser,
            "procurement",
        )
        await gotoPage(procurementPage, "/workspace")
        await openWorkspaceTask(procurementPage, order1Id)

        // ---------- 驳回并填写原因（UI 无入口，见头部注释；经决策 API） ----------
        const REJECT_REASON = "成本上涨，请重新报价后再提交"
        const decision = await submitApprovalDecisionViaApi(
            request,
            procurementToken,
            order1Id,
            "REJECT",
            REJECT_REASON,
        )
        // 协议层事实（当前构建仅回显，不推进实例——见 risks）
        expect(decision.outcome).toBe("APPLIED")
        expect(decision.latest_rejection_reason).toBe(REJECT_REASON)

        // ---------- 销售视角：销售单不生效、内容不变 ----------
        const { page: salesPage } = await newLoggedInContext(browser, "sales")
        await gotoPage(salesPage, `/sales/orders/${order1Id}`)
        await expect(salesPage.getByText("审批中", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(salesPage.getByText("已生效", { exact: true })).toHaveCount(0)
        await expect(salesPage.getByText(/1,?000\.00/).first()).toBeVisible()
        const linesAfterReject = await salesPage
            .locator("table")
            .filter({ hasText: skuName })
            .first()
            .innerText()
        expect(linesAfterReject).toBe(order1LinesBeforeReject)
        // 审批区入口存在（CANCEL 动作来自单据层；弹窗依赖 instance 恒为 null，见 doc_mismatches）
        await expect(
            salesPage.getByRole("button", { name: "撤回审批" }).first(),
        ).toBeVisible({ timeout: 20_000 })

        // ---------- 审批轮次 +1 且回到首节点（审批实例 API 佐证） ----------
        // 预期: 驳回后进入第 2 轮、节点回到「采购确认」。
        // 当前构建决策接口为桩实现，轮次不会推进 → 本断言失败即暴露该缺口（见 doc_mismatches/risks）。
        const inst = await findApprovalInstance(request, salesToken, order1Id)
        expect(
            inst.current_round_no,
            "驳回后审批轮次应加一（当前构建 decision API 为桩实现，不会推进轮次，见 risks）",
        ).toBe(2)
        expect(inst.current_node_name).toBe("采购确认")

        // 执行历史：第 1 轮 REJECTED、第 2 轮 ACTIVE（同样依赖决策接线，当前构建失败）
        const history = await api<ApprovalHistoryExecution[]>(
            request,
            "GET",
            `/admin/approval-instances/${inst.instance_id}/history`,
            { token: salesToken },
        )
        expect(
            history.some(
                (item) => item.current_round_no === 1 && item.status === "REJECTED",
            ),
            "历史应含第 1 轮 REJECTED 执行",
        ).toBe(true)
        expect(
            history.some(
                (item) =>
                    item.current_round_no === 2 &&
                    item.status === "ACTIVE" &&
                    item.current_node_name === "采购确认",
            ),
            "历史应含第 2 轮 ACTIVE 首节点执行",
        ).toBe(true)
    })

    test("03 场景A：照原条件承接——第 2 轮重新通过后销售单生效", async ({
        browser,
        request,
    }) => {
        expect(order1Id, "前置 test 01 必须已创建销售单").toBeTruthy()
        const salesToken = await apiLogin(request, "sales")
        const procurementToken = await apiLogin(request, "procurement")

        // ---------- 不撤回不改单：采购工作台出现第 2 轮任务 ----------
        const { page: procurementPage } = await newLoggedInContext(
            browser,
            "procurement",
        )
        await gotoPage(procurementPage, "/workspace")
        await openWorkspaceTask(procurementPage, order1Id)

        // ---------- 对当前开放任务提交「通过」 ----------
        const decision = await submitApprovalDecisionViaApi(
            request,
            procurementToken,
            order1Id,
            "APPROVE",
        )
        expect(decision.outcome).toBe("APPLIED")

        // 实例最终通过（当前构建决策接口为桩实现，实例停在 RUNNING → 本断言失败即暴露缺口）
        await expect
            .poll(
                async () => (await findApprovalInstance(request, salesToken, order1Id)).status,
                {
                    timeout: 20_000,
                    message:
                        "第 2 轮通过后实例应 APPROVED（当前构建 decision API 为桩实现，见 doc_mismatches/risks）",
                },
            )
            .toBe("APPROVED")

        // ---------- 销售视角：销售单生效 ----------
        const { page: salesPage } = await newLoggedInContext(browser, "sales")
        await gotoPage(salesPage, `/sales/orders/${order1Id}`)
        await expect(salesPage.getByText("已生效", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(salesPage.getByText("审批中", { exact: true })).toHaveCount(0)
    })

    test("04 场景B：驳回后销售撤回审批回草稿并作废", async ({ browser, request }) => {
        expect(order2Id, "前置 test 01 必须已创建销售单").toBeTruthy()
        const salesToken = await apiLogin(request, "sales")
        const procurementToken = await apiLogin(request, "procurement")

        // ---------- 采购在工作台处理第二张单：驳回并填写原因 ----------
        const { page: procurementPage } = await newLoggedInContext(
            browser,
            "procurement",
        )
        await gotoPage(procurementPage, "/workspace")
        await openWorkspaceTask(procurementPage, order2Id)
        const REJECT_REASON2 = "交期无法满足，客户确认后请重新提交"
        const decision = await submitApprovalDecisionViaApi(
            request,
            procurementToken,
            order2Id,
            "REJECT",
            REJECT_REASON2,
        )
        expect(decision.outcome).toBe("APPLIED")
        expect(decision.latest_rejection_reason).toBe(REJECT_REASON2)

        // ---------- 销售视角：仍审批中、金额未变 ----------
        const { page: salesPage } = await newLoggedInContext(browser, "sales")
        await gotoPage(salesPage, `/sales/orders/${order2Id}`)
        await expect(salesPage.getByText("审批中", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(salesPage.getByText(/1,?200\.00/).first()).toBeVisible()

        // ---------- 撤回审批（UI 弹窗缺失，见 doc_mismatches；走已接线的撤回接口） ----------
        const detail2 = await api<BackendSalesOrderDetail>(
            request,
            "GET",
            `/admin/sales-orders/${order2Id}`,
            { token: salesToken },
        )
        expect(detail2.commercial_status).toBe("PENDING_REVIEW")
        await api(
            request,
            "POST",
            `/admin/sales-orders/${order2Id}/cancel-approval`,
            {
                token: salesToken,
                body: {
                    expected_version: detail2.version,
                    reason: "客户取消，不再继续办理审批",
                    idempotency_key: `flow05-cancel-${order2Id}`,
                },
            },
        )
        // 实例终态 CANCELLED（撤回已接线，当前构建应通过）
        await expect
            .poll(
                async () => (await findApprovalInstance(request, salesToken, order2Id)).status,
                { timeout: 20_000 },
            )
            .toBe("CANCELLED")

        // ---------- 销售视角：回到草稿（可编辑，可再次提交） ----------
        await gotoPage(salesPage, `/sales/orders/${order2Id}`)
        await expect(salesPage.getByText("草稿", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(
            salesPage.getByRole("button", { name: "提交", exact: true }).first(),
        ).toBeVisible({ timeout: 20_000 })

        // ---------- 作废销售单草稿（UI 无作废入口，见 doc_mismatches；DRAFT → VOIDED 已接线） ----------
        const draft2 = await api<BackendSalesOrderDetail>(
            request,
            "GET",
            `/admin/sales-orders/${order2Id}`,
            { token: salesToken },
        )
        expect(draft2.commercial_status).toBe("DRAFT")
        await api(request, "POST", `/admin/sales-orders/${order2Id}/void`, {
            token: salesToken,
            body: { version: draft2.version },
        })

        // ---------- 销售视角：已作废（只读终态） ----------
        await gotoPage(salesPage, `/sales/orders/${order2Id}`)
        await expect(salesPage.getByText("已作废", { exact: true }).first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(salesPage.getByText("审批中", { exact: true })).toHaveCount(0)
    })
})

// ---------------------------------------------------------------------------
// 建单并提交（客户/合同已存在，从销售单列表进入建单页）
// ---------------------------------------------------------------------------

async function createAndSubmitSalesOrder(input: {
    salesPage: Page
    contractNo: string
    customerName: string
    skuNo: string
    skuName: string
    settlementParty?: { id: string; party_no: string } | undefined
    quantity: string
    unitPrice: string
    expectedGross: RegExp
    /** 首次建单上传合同 PDF；后续建单直接选择既有合同（合同编号唯一，不可重复上传）。 */
    uploadContract: boolean
}): Promise<{ id: string }> {
    const {
        salesPage,
        contractNo,
        customerName,
        skuNo,
        skuName,
        settlementParty,
        quantity,
        unitPrice,
        expectedGross,
        uploadContract,
    } = input

    await gotoPage(salesPage, "/sales/orders?mode=create")

    // 选择合同：首次上传 PDF 并归档；再次建单直接搜索既有合同
    if (uploadContract) {
        await salesPage.getByRole("button", { name: "上传合同 PDF" }).first().click()
        const uploadDialog = salesPage.getByRole("dialog").last()
        await expect(uploadDialog).toBeVisible({ timeout: 20_000 })
        await uploadDialog.locator('input[type="file"]').setInputFiles(CONTRACT_PDF)
        await uploadDialog.getByLabel("合同编号").fill(contractNo)
        await pickComboboxOption(salesPage, uploadDialog, "客户", customerName)
        if (settlementParty) {
            await pickComboboxOption(
                salesPage,
                uploadDialog,
                "结算主体",
                settlementParty.party_no,
            )
        } else {
            // 无可用主体时选弹层第一项（回退）
            await pickFirstComboboxOption(salesPage, "结算主体")
        }
        await pickComboboxOption(salesPage, uploadDialog, "付款条件", "按合同约定")
        await pickDate(salesPage, "签订日期", futureDate(0))
        await pickDate(salesPage, "有效期起", futureDate(0))
        await pickDate(salesPage, "有效期止", futureDate(365))
        await uploadDialog.getByRole("button", { name: "上传并归档" }).click()
        await expect(uploadDialog).not.toBeVisible({ timeout: 20_000 })
    } else {
        await pickComboboxOption(salesPage, salesPage, "合同", contractNo)
    }

    // 等待合同信息同步（带出客户/结算主体/付款条件）
    await expect(
        salesPage.getByText(new RegExp(contractNo)).first(),
    ).toBeVisible({ timeout: 20_000 })

    // 单据头
    await pickComboboxOption(salesPage, salesPage, "福利场景", "年节礼包")
    await pickDate(salesPage, "履约期限", futureDate(90))

    // 销售明细（一条实物/服务行）
    await pickComboboxOption(salesPage, salesPage, "商品", skuNo)
    await salesPage.getByLabel("数量").fill(quantity)
    await salesPage.getByLabel("含税单价").fill(unitPrice)
    await pickDate(salesPage, "交付日期", futureDate(60))
    // 商品名作为输入值而非文本节点存在，按 combobox 值断言行已带出商品
    await expect(salesPage.getByRole("combobox", { name: "商品" }).first()).toHaveValue(
        new RegExp(skuName),
    )

    // 提交并确认
    await salesPage.getByRole("button", { name: "提交", exact: true }).click()
    const submitDialog = salesPage.getByRole("alertdialog").last()
    await expect(submitDialog).toBeVisible({ timeout: 20_000 })
    await submitDialog.getByRole("button", { name: "确认提交" }).click()

    // 提交成功进入详情页
    await expect(salesPage).toHaveURL(/\/sales\/orders\/[^/?]+/, {
        timeout: 30_000,
    })
    const orderId = new URL(salesPage.url()).pathname.split("/").pop()!
    await expect(salesPage.getByText(expectedGross).first()).toBeVisible({
        timeout: 20_000,
    })
    return { id: orderId }
}
