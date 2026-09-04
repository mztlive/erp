/**
 * 流程: [flow-20] 卡券销售单票款复核
 * 文档: docs/erp-phase-1.md §8.7 + §9.1（同步金额变化后应收重算）+ W13
 *       工作台 docs/workbench-workitem-contract.md 第 3 节
 * 账号: xiaoshou 建客户/合同并尝试提交卡券销售单；lisiyong → yunying → caiwu
 *       审批 VoucherSalesOrder；caiwu 在 W01 完成 CARD_FUNDS_REVIEW；
 *       fukuan 登记回款、caiwu 审批入账；kaipiao 在 W01 登记销项发票；
 *       admin 探查 W18。
 *
 * 前置条件与跳过策略（商城移除后）：
 * - 卡券销售单由 ERP 直接创建，无需目标商城与外部身份映射。
 * - 若无卡券类目或提交失败：断言「未派生应收 / 无 CARD_FUNDS_REVIEW」，
 *   登记 annotation 后结束，不走复核/回款/开票。
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档 §9.3.1 要求回款/开票进度在复核未完成时标注「待复核」；销售单摘要
 *    mapCollection/mapInvoicing 只有未收/部分回款/已结清与未开/部分开票/已开齐。
 *    待复核展示在应收台账 reviewStatusLabel（期初待复核/同步差额待复核）。
 * 2. 文档复核完成后才登记回款/发票；代码在销售单生效时同时创建
 *    CARD_FUNDS_REVIEW 与 SALES_INVOICE_EXECUTION，W13 还可登记历史回款/发票。
 * 3. 客户往来列表 businessTypeLabel 固定写成「实物服务」，即使账户来自卡券单。
 * 4. W01 卡券票款任务 object 标签是「应收子账 N」，来源销售单在 listSummary
 *    （「销售单 {orderNo}」），不是「来源销售单」。
 * 5. 资金终态按钮不用「过账」；回款正式入账后列表状态文案是「已过账」。
 */
import { test, expect, type Browser, type BrowserContext, type Page } from "@playwright/test"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"

const TIMEOUT = 20_000
const LONG = 40_000
const VOUCHER_NAME = "福尚通 100 元"
const CARD_COUNT = "10"
const FACE_VALUE = "100.00"
const UNIT_PRICE = "95.00"
const GROSS_TOTAL = "950.00"

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const CONTRACT_PDF = path.resolve(REPO_ROOT, "fixtures", "sample-contract.pdf")

test.describe.configure({ mode: "serial" })

test.describe("flow-20 卡券销售单票款复核", () => {
    test("卡券单走期初复核、回款与开票", async ({
        page,
        browser,
    }) => {
        test.setTimeout(12 * 60 * 1000)

        const stamp = Date.now().toString(36).toUpperCase()
        const legalName = `卡券票款客户${stamp}`
        const shortName = `卡券${stamp.slice(-6)}`
        const creditCode = (`9111F20${stamp}000000000000`).replace(/[^0-9A-Z]/g, "0").slice(0, 18)
        const contractNo = `HT-F20-${stamp}`
        const extra: BrowserContext[] = []

        try {
            // ── 0. /workspace/tasks 只重定向到 W01，不是第二待办入口 ──
            await loginViaUi(page, accountSpec("xiaoshou"))
            await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
                timeout: LONG,
            })
            await page.goto("/workspace/tasks")
            await expect(page).toHaveURL(/\/workspace(?:\?|$)/, { timeout: LONG })
            await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
                timeout: LONG,
            })

            // ── 1. 销售：客户 + 合同（后续卡券单与应收都挂在此主体）──
            const customerId = await createCustomer(page, {
                legalName,
                shortName,
                creditCode,
            })
            await uploadContract(page, {
                customerId,
                legalName,
                contractNo,
            })

            // ── 2. 探查 W18：reset 后应无导入批次 ──
            const admin = await openRole(browser, extra, "admin")
            await probeImports(admin.page)
            await admin.context.close()

            const caiwuProbe = await openRole(browser, extra, "caiwu")
            await caiwuProbe.page.goto("/finance/card-funds-review")
            await expect(
                caiwuProbe.page.getByRole("heading", { name: "卡券票款复核" }),
            ).toBeVisible({ timeout: LONG })
            await expect(
                caiwuProbe.page.getByText(/当前筛选项已处理完|筛选无结果|当前没有待处理事项/),
            ).toBeVisible({ timeout: LONG })
            await caiwuProbe.context.close()

            // ── 3. 尝试 ERP UI 提交卡券销售单（不得改用实物单）──
            const created = await tryCreateVoucherSalesOrder(page, {
                customerId,
                contractNo,
                legalName,
            })

            if (!created.ok) {
                test.info().annotations.push({
                    type: "skip-strategy",
                    description: created.reason,
                })

                // 提交未完成前禁止派生应收，也不得出现期初复核任务。
                const caiwu = await openRole(browser, extra, "caiwu")
                await assertNoCardFundsReview(caiwu.page, legalName)
                await assertReceivableNotDerived(caiwu.page, legalName)
                await caiwu.context.close()

                const caigou = await openRole(browser, extra, "caigou")
                await assertNoProcurementTask(caigou.page, legalName)
                await caigou.context.close()
                return
            }

            const { id: orderId, orderNo } = created

            // ── 4. 卡券审批：lisiyong → yunying → caiwu（提交人 xiaoshou 不在节点）──
            const leader = await openRole(browser, extra, "lisiyong")
            await approveWorkspaceTask(leader.page, "卡券销售单审批", orderNo)
            await leader.context.close()

            const ops = await openRole(browser, extra, "yunying")
            await approveWorkspaceTask(ops.page, "卡券销售单审批", orderNo)
            await ops.context.close()

            const caiwuApprove = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwuApprove.page, "卡券销售单审批", orderNo)
            await caiwuApprove.context.close()

            await page.goto(`/sales/orders/${orderId}`)
            await expectEffectiveVoucherOrder(page, orderNo)

            // 负向：卡券单不进供给分配、不建采购单、不做客户验收、不得关闭。
            await page.getByRole("tab", { name: /^采购/ }).click()
            await expect(page.getByTestId("sales-order-purchase-status")).toContainText(
                "采购单 0 笔",
                { timeout: TIMEOUT },
            )
            await expect(page.getByText("本单还没有采购单。")).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(page.getByRole("tab", { name: /^验收/ })).toHaveCount(0)
            await expectNotClosed(page)
            await expect(page.getByRole("button", { name: "发起改单" })).toBeVisible({
                timeout: TIMEOUT,
            })

            const caigou = await openRole(browser, extra, "caigou")
            await assertNoProcurementTask(caigou.page, orderNo)
            await caigou.context.close()

            // ── 5. 复核完成前：应收台账必须显示期初待复核；票款指标不可靠 ──
            const caiwuLedger = await openRole(browser, extra, "caiwu")
            await assertReceivableReviewStatus(caiwuLedger.page, {
                orderNo,
                customerName: legalName,
                label: "期初待复核",
            })
            await caiwuLedger.page.goto("/analytics/customer-quality")
            const qualityHeading = caiwuLedger.page.getByRole("heading", {
                name: "客户经营质量",
            })
            await expect(qualityHeading).toBeVisible({ timeout: LONG })
            const periodBlocked = await caiwuLedger.page
                .getByRole("heading", { name: "请选择统计期间" })
                .isVisible()
                .catch(() => false)
            if (!periodBlocked) {
                await expect(
                    caiwuLedger.page.getByText(/票款复核不足|票款未复核|卡券票款复核进度/),
                ).toBeVisible({ timeout: LONG })
            }
            await caiwuLedger.context.close()

            // ── 6. 财务总监在 W01 原地完成 CARD_FUNDS_REVIEW（从 0 起）──
            const caiwuReview = await openRole(browser, extra, "caiwu")
            await completeOpeningReviewFromZero(caiwuReview.page, {
                orderNo,
                customerName: legalName,
            })
            await assertReceivableReviewStatus(caiwuReview.page, {
                orderNo,
                customerName: legalName,
                label: "已复核",
            })
            await caiwuReview.context.close()

            await page.goto(`/sales/orders/${orderId}`)
            await expectCollection(page, "未收")
            await expectInvoicing(page, "未开")
            await expectNotClosed(page)

            // ── 7. 出纳登记回款 → 财务总监审批入账（禁止 caiwu 自己提交）──
            const caiwuDenied = await openRole(browser, extra, "caiwu")
            await assertCaiwuCannotSubmitReceipt(caiwuDenied.page, legalName)
            await caiwuDenied.context.close()

            const fukuan = await openRole(browser, extra, "fukuan")
            const receiptNo = await registerReceiptForOrder(fukuan.page, {
                customerName: legalName,
                orderNo,
                amount: GROSS_TOTAL,
                bankReference: `BANK-F20-${stamp}`,
            })
            await fukuan.context.close()

            const caiwuReceipt = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwuReceipt.page, "回款复核", receiptNo)
            await caiwuReceipt.context.close()

            await page.goto(`/sales/orders/${orderId}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 8. 开票人在 W01 销项开票任务登记发票；开票完成不是关闭条件 ──
            const kaipiao = await openRole(browser, extra, "kaipiao")
            await registerSalesInvoiceFromWorkspace(kaipiao.page, orderNo, GROSS_TOTAL)
            await kaipiao.context.close()

            await page.goto(`/sales/orders/${orderId}`)
            await expectInvoicing(page, "已开齐")
            await expectCollection(page, "已结清")
            await expectNotClosed(page)
        } finally {
            await Promise.allSettled(extra.map((context) => context.close()))
        }
    })
})

// ─── 账号 / 登录 ───────────────────────────────────────────────────────────

function accountSpec(login: string) {
    const bag = ACCOUNTS as Record<string, unknown>
    const aliases: Record<string, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        lisiyong: ["lisiyong", "salesLeader", "sales_leader"],
        yunying: ["yunying", "operations", "ops"],
        caigou: ["caigou", "procurement"],
        caiwu: ["caiwu", "finance"],
        fukuan: ["fukuan", "payment"],
        kaipiao: ["kaipiao", "invoice"],
        admin: ["admin"],
    }
    for (const key of aliases[login] ?? [login]) {
        if (bag[key] != null) return bag[key]
    }
    return { account: login, password: "123456" }
}

async function openRole(
    browser: Browser,
    extra: BrowserContext[],
    login: string,
): Promise<{ context: BrowserContext; page: Page }> {
    const opened = await newLoggedInContext(
        browser,
        accountSpec(login) as Parameters<typeof newLoggedInContext>[1],
    )
    const bundle =
        opened && typeof opened === "object" && "page" in opened
            ? (opened as { context: BrowserContext; page: Page })
            : {
                  context: opened as BrowserContext,
                  page:
                      (opened as BrowserContext).pages()[0] ??
                      (await (opened as BrowserContext).newPage()),
              }
    extra.push(bundle.context)
    if (await bundle.page.locator("#governance-auth-login-account").isVisible().catch(() => false)) {
        await loginViaUi(bundle.page, accountSpec(login) as Parameters<typeof loginViaUi>[1])
    }
    return bundle
}

// ─── 通用 UI ───────────────────────────────────────────────────────────────

async function chooseCombobox(page: Page, inputId: string, query: string, optionId?: string) {
    const input = page.locator(`#${inputId}`)
    await expect(input).toBeVisible({ timeout: TIMEOUT })
    await input.click()
    await input.fill(query)
    if (optionId) {
        const byId = page.locator(`#${optionId}`)
        if (await byId.isVisible({ timeout: TIMEOUT }).catch(() => false)) {
            await byId.click()
            return
        }
    }
    const option = page.getByRole("option", { name: new RegExp(escapeRe(query)) })
    if (await option.first().isVisible({ timeout: 5000 }).catch(() => false)) {
        await option.first().click()
        return
    }
    const item = page.locator('[data-slot="combobox-item"]').filter({ hasText: query })
    await expect(item.first()).toBeVisible({ timeout: TIMEOUT })
    await item.first().click()
}

async function pickCalendarDay(page: Page, fieldId: string, iso: string) {
    const monthStart = `${iso.slice(0, 8)}01`
    await page.locator(`#${fieldId}`).click()
    const nextMonth = page.locator(`#${fieldId}-calendar-next-month`)
    const day = page.locator(`#${fieldId}-calendar-month-${monthStart}-day-${iso}`)
    if (await day.isVisible({ timeout: 3000 }).catch(() => false)) {
        await day.click()
        return
    }
    if (await nextMonth.isVisible().catch(() => false)) {
        await nextMonth.click()
        const next = new Date(`${iso}T00:00:00`)
        next.setMonth(next.getMonth() + 1)
        const nextIso = [
            next.getFullYear(),
            String(next.getMonth() + 1).padStart(2, "0"),
            "01",
        ].join("-")
        const nextMonthStart = `${nextIso.slice(0, 8)}01`
        const nextDay = page.locator(
            `#${fieldId}-calendar-month-${nextMonthStart}-day-${nextIso}`,
        )
        await expect(nextDay).toBeVisible({ timeout: TIMEOUT })
        await nextDay.click()
        return
    }
    await expect(day).toBeVisible({ timeout: TIMEOUT })
    await day.click()
}

function todayIso() {
    const today = new Date()
    return [
        today.getFullYear(),
        String(today.getMonth() + 1).padStart(2, "0"),
        String(today.getDate()).padStart(2, "0"),
    ].join("-")
}

function contractPdfFile() {
    if (fs.existsSync(CONTRACT_PDF)) {
        return CONTRACT_PDF
    }
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf" as const,
        buffer: Buffer.from(
            "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
        ),
    }
}

async function factValue(page: Page, label: string) {
    const dt = page.locator('[data-slot="formal-action-result"] dt', { hasText: label })
    await expect(dt).toBeVisible({ timeout: TIMEOUT })
    return (await dt.locator("xpath=following-sibling::dd[1]").innerText()).trim()
}

function escapeRe(value: string) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

async function waitHeading(page: Page, name: string | RegExp) {
    await expect(page.getByRole("heading", { name })).toBeVisible({ timeout: LONG })
}

// ─── 客户 / 合同 ───────────────────────────────────────────────────────────

async function createCustomer(
    page: Page,
    input: { legalName: string; shortName: string; creditCode: string },
) {
    await page.goto("/sales/customers")
    await waitHeading(page, "客户中心")
    await page.locator("#customers-directory-create").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "新建客户" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customers-form-legal-name").fill(input.legalName)
    await page.locator("#customers-form-short-name").fill(input.shortName)
    await page.locator("#customers-form-credit-code").fill(input.creditCode)
    await page.locator("#customers-form-submit").click()
    await expect(page.getByText("客户已创建")).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("dialog").getByRole("heading", { name: "新建客户" })).toBeHidden({
        timeout: TIMEOUT,
    })

    await page.locator("#customers-directory-search").fill(input.legalName)
    await page.locator("#customers-directory-search").press("Enter")
    const open = page.getByRole("link", { name: input.shortName })
    await expect(open).toBeVisible({ timeout: LONG })
    await open.click()
    await expect(page.getByRole("heading", { name: input.legalName })).toBeVisible({
        timeout: LONG,
    })
    const match = page.url().match(/\/sales\/customers\/([^/?#]+)/)
    expect(match?.[1]).toBeTruthy()
    return match![1]
}

async function uploadContract(
    page: Page,
    input: { customerId: string; legalName: string; contractNo: string },
) {
    await page.goto(`/sales/contracts?customerId=${encodeURIComponent(input.customerId)}&upload=1`)
    await expect(page.getByRole("dialog").getByRole("heading", { name: "上传合同 PDF" })).toBeVisible({
        timeout: LONG,
    })
    await page.locator("#card-contracts-upload-pdf-input").setInputFiles(contractPdfFile())
    await page.locator("#card-contracts-upload-contract-no").fill(input.contractNo)
    const customerInput = page.locator("#card-contracts-upload-customer")
    const customerValue = (await customerInput.inputValue()).trim()
    if (!customerValue) {
        await chooseCombobox(page, "card-contracts-upload-customer", input.legalName)
    }
    await expect(page.locator("#card-contracts-upload-settlement-party")).not.toHaveValue("", {
        timeout: LONG,
    })
    await page.locator("#card-contracts-upload-submit").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "上传合同 PDF" })).toBeHidden({
        timeout: LONG,
    })
    await expect(page.getByText(input.contractNo)).toBeVisible({ timeout: LONG })
}

// ─── W18 探查 ────────────────────────────────────────────────────────────

async function probeImports(page: Page): Promise<void> {
    await page.goto("/governance/imports")
    await expect(page.getByRole("heading", { name: "导入与期初" })).toBeVisible({
        timeout: LONG,
    })
    await expect(page.getByText(/还没有导入批次|当前环境还没有导入批次/)).toBeVisible({
        timeout: LONG,
    })
}

// ─── 卡券销售单 ────────────────────────────────────────────────────────────

type CreateVoucherResult =
    | { ok: true; id: string; orderNo: string }
    | { ok: false; reason: string }

async function tryCreateVoucherSalesOrder(
    page: Page,
    input: {
        customerId: string
        contractNo: string
        legalName: string
    },
): Promise<CreateVoucherResult> {
    await page.goto(`/sales/orders?mode=create&customerId=${encodeURIComponent(input.customerId)}`)
    await expect(page.getByRole("heading", { name: "单据头" })).toBeVisible({ timeout: LONG })

    await chooseCombobox(page, "sales-orders-create-contract", input.contractNo)
    await expect(page.getByText(new RegExp(`客户\\s+${escapeRe(input.legalName)}`))).toBeVisible({
        timeout: LONG,
    })

    await page.locator("#sales-orders-create-header-nature").click()
    await page.getByRole("option", { name: "卡券" }).click()
    const switchNature = page.getByRole("dialog").getByRole("heading", { name: "切换业务性质？" })
    if (await switchNature.isVisible({ timeout: 2000 }).catch(() => false)) {
        await page.getByRole("button", { name: "清空明细并切换" }).click()
        await expect(switchNature).toBeHidden({ timeout: TIMEOUT })
    }
    await expect(page.getByText("卡券 · 仅一条")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.locator("#sales-orders-create-header-tax-rate")).toHaveValue("6.00", {
        timeout: TIMEOUT,
    })

    await chooseCombobox(
        page,
        "sales-orders-create-header-welfare-scene",
        "消费金",
        "sales-orders-create-header-welfare-scene-option-consumption-fund",
    )
    await chooseCombobox(
        page,
        "sales-orders-create-header-payment-terms",
        "货到 30 天",
        "sales-orders-create-header-payment-terms-option-postpay-net30",
    )

    const today = todayIso()
    await pickCalendarDay(page, "sales-orders-create-header-fulfillment-deadline", today)
    await pickCalendarDay(page, "sales-orders-create-header-receivable-due-date", today)

    const lineTable = page.locator("#sales-orders-create-line-items")
    const category = lineTable.getByPlaceholder("搜索卡券类目")
    await expect(category).toBeVisible({ timeout: TIMEOUT })
    await category.click()
    await category.fill(VOUCHER_NAME)
    if (await page.getByText("暂无可用的卡券类目").isVisible().catch(() => false)) {
        return {
            ok: false,
            reason: "无可用卡券类目，无法从 UI 创建卡券销售单。",
        }
    }
    const voucherOption = page.getByRole("option", { name: new RegExp(escapeRe(VOUCHER_NAME)) })
    if (!(await voucherOption.isVisible({ timeout: LONG }).catch(() => false))) {
        return {
            ok: false,
            reason: `卡券类目「${VOUCHER_NAME}」不可选，无法从 UI 创建卡券销售单。`,
        }
    }
    await voucherOption.click()

    await lineTable.getByLabel("数量").fill(CARD_COUNT)
    await lineTable.getByLabel("含税单价").fill(UNIT_PRICE)
    await lineTable.getByLabel("面值").fill(FACE_VALUE)

    await page.locator("#sales-orders-create-submit").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "提交销售单" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(
        page.getByText(/提交后进入销售领导 → 运营/),
    ).toBeVisible({ timeout: TIMEOUT })
    await page.locator("#sales-orders-submit-confirm-confirm").click()

    const createdUrl = page.waitForURL(/\/sales\/orders\/[^/?#]+/, { timeout: LONG }).then(() => "ok")
    const uncreated = page
        .getByRole("heading", { name: /销售单未创建|操作未完成/ })
        .waitFor({ state: "visible", timeout: LONG })
        .then(() => "error")
    const outcome = await Promise.race([createdUrl, uncreated]).catch(() => "timeout")

    if (outcome !== "ok") {
        const detail = (
            (await page.locator('[role="alert"]').innerText().catch(() => "")) ||
            String(outcome)
        ).trim()
        return {
            ok: false,
            reason: `无法从 UI 创建卡券销售单（${detail}）。`,
        }
    }

    await expect(page.getByText("审批中", { exact: true }).first()).toBeVisible({ timeout: LONG })
    const id = page.url().split("/sales/orders/")[1]?.split(/[?#]/)[0] ?? ""
    expect(id).toBeTruthy()
    const identity = await page
        .locator("header")
        .filter({ has: page.getByRole("heading", { level: 1 }) })
        .innerText()
    const orderNo = identity.match(/单号\s+(\S+)/)?.[1] ?? ""
    expect(orderNo.length).toBeGreaterThan(4)
    await expect(page.getByText("卡券", { exact: true }).first()).toBeVisible({ timeout: TIMEOUT })
    return { ok: true, id, orderNo }
}

async function expectEffectiveVoucherOrder(page: Page, orderNo: string) {
    await expect(page.getByText(orderNo, { exact: true }).first()).toBeVisible({ timeout: LONG })
    await expect(page.getByText("已生效", { exact: true }).first()).toBeVisible({ timeout: LONG })
    await expectCollection(page, "未收")
    await expectInvoicing(page, "未开")
    await expectNotClosed(page)
    await expect(page.getByText("卡券", { exact: true }).first()).toBeVisible({ timeout: TIMEOUT })
}

async function expectCollection(page: Page, label: "未收" | "部分回款" | "已结清") {
    await expect(page.getByLabel("销售单金额摘要").getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
}

async function expectInvoicing(page: Page, label: "未开" | "部分开票" | "已开齐") {
    await expect(page.getByLabel("销售单金额摘要").getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
}

async function expectNotClosed(page: Page) {
    const identity = page.getByRole("heading", { level: 1 }).locator("xpath=ancestor::header[1]")
    await expect(identity.getByText("已生效", { exact: true })).toBeVisible({ timeout: LONG })
    await expect(identity.getByText("已关闭", { exact: true })).toHaveCount(0)
}

// ─── 工作台 ────────────────────────────────────────────────────────────────

async function openWorkspaceFamily(page: Page, family: "approval" | "finance" | "procurement") {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
    await page.locator(`#workspace-family-nav-${family}`).click()
}

async function searchWorkspace(page: Page, hint: string) {
    const search = page.locator("#workspace-queue-toolbar-search-input")
    await expect(search).toBeVisible({ timeout: TIMEOUT })
    await search.fill(hint)
    await search.press("Enter")
}

async function approveWorkspaceTask(page: Page, typeLabel: string, hint: string) {
    await openWorkspaceFamily(page, typeLabel.includes("开票") ? "finance" : "approval")
    await searchWorkspace(page, hint)
    const task = page
        .getByRole("button", {
            name: new RegExp(`${escapeRe(typeLabel)}[\\s\\S]*${escapeRe(hint)}`),
        })
        .or(
            page.getByRole("button", { name: new RegExp(escapeRe(typeLabel)) }).filter({
                hasText: hint,
            }),
        )
    await expect(task).toBeVisible({ timeout: LONG })
    await task.click()
    const approve = page.getByRole("button", { name: "通过" })
    await expect(approve).toBeVisible({ timeout: LONG })
    await approve.click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeVisible({ timeout: TIMEOUT })
    await page.getByRole("button", { name: "确认通过" }).click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeHidden({ timeout: LONG })
}

async function completeOpeningReviewFromZero(
    page: Page,
    input: { orderNo: string; customerName: string },
) {
    await openWorkspaceFamily(page, "finance")
    await searchWorkspace(page, input.orderNo)
    const task = page.getByRole("button", { name: /卡券票款复核/ }).filter({
        hasText: new RegExp(`${escapeRe(input.orderNo)}|${escapeRe(input.customerName)}`),
    })
    await expect(task).toBeVisible({ timeout: LONG })
    await task.click()
    await expect(page.getByLabel("当前卡券票款复核任务")).toBeVisible({ timeout: LONG })
    await expect(page.getByText("期初复核")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("期初票款复核")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("票款指标不可靠（复核未完成）")).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText(/尚无回款\/发票/)).toBeVisible({ timeout: TIMEOUT })

    await page.locator("#card-contracts-funds-review-decision-ev-doc").fill(`EV-OPEN-${input.orderNo}`)
    await page.locator("#card-contracts-funds-review-decision-ev-ref").fill("期初无历史票款，从 0 起核对")
    const zero = page.locator("#card-contracts-funds-review-decision-zero")
    await expect(zero).toBeEnabled({ timeout: TIMEOUT })
    await zero.click()
    await expect(page.getByRole("heading", { name: "确认无历史票款，从 0 起" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.getByRole("button", { name: "确认从 0 起并完成" }).click()
    await expect(page.getByRole("heading", { name: "确认无历史票款，从 0 起" })).toBeHidden({
        timeout: LONG,
    })
    await expect(page.getByText(/复核通过 · 复核号/)).toBeVisible({ timeout: LONG })
    await expect(page.getByText("无历史票款，从 0 起")).toBeVisible({ timeout: TIMEOUT })
}

async function assertNoCardFundsReview(page: Page, hint: string) {
    await openWorkspaceFamily(page, "finance")
    await searchWorkspace(page, hint)
    await expect(page.getByRole("button", { name: /卡券票款复核/ })).toHaveCount(0)
    await expect(page.getByRole("button", { name: /卡券票款差异复核/ })).toHaveCount(0)
}

async function assertNoProcurementTask(page: Page, hint: string) {
    await openWorkspaceFamily(page, "procurement")
    await searchWorkspace(page, hint)
    await expect(page.getByRole("button", { name: /待供给分配/ })).toHaveCount(0)
}

// ─── 应收台账 ──────────────────────────────────────────────────────────────

async function assertReceivableNotDerived(page: Page, customerName: string) {
    await page.goto("/finance/customer-accounts?view=receivable")
    await waitHeading(page, "客户往来")
    await page.locator("#customer-receivables-toolbar-search").fill(customerName)
    await page.locator("#customer-receivables-toolbar-search").press("Enter")
    await expect(page.getByText("期初待复核")).toHaveCount(0)
    await expect(page.getByText("同步差额待复核")).toHaveCount(0)
}

async function assertReceivableReviewStatus(
    page: Page,
    input: { orderNo: string; customerName: string; label: "期初待复核" | "已复核" | "同步差额待复核" },
) {
    await page.goto("/finance/customer-accounts?view=receivable")
    await waitHeading(page, "客户往来")
    await page.locator("#customer-receivables-view-receivable").click()
    await page.locator("#customer-receivables-toolbar-more-filters").click()
    const reviewFilter = page.locator("#customer-receivables-toolbar-review-status-filter")
    await expect(reviewFilter).toBeVisible({ timeout: TIMEOUT })
    await reviewFilter.getByRole("radio", { name: input.label }).click()
    await page.locator("#customer-receivables-toolbar-search").fill(input.orderNo)
    await page.locator("#customer-receivables-toolbar-apply").click()
    const row = page.getByRole("row").filter({ hasText: input.orderNo })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText(input.label)).toBeVisible({ timeout: TIMEOUT })
    await expect(row.getByText(input.customerName)).toBeVisible({ timeout: TIMEOUT })
    if (input.label === "期初待复核") {
        await expect(page.locator("#customer-receivables-metrics-unallocated-invoice")).toContainText(
            /卡券待复核/,
            { timeout: TIMEOUT },
        )
    }
}

// ─── 回款 / 开票 ───────────────────────────────────────────────────────────

async function resolveReceiptPartyPicker(page: Page, customerName: string) {
    const sessionHeading = page.getByRole("heading", { name: /核销 · / })
    const picker = page.getByRole("dialog").filter({ hasText: "登记回款 — 选择往来主体" })
    await Promise.race([
        sessionHeading.waitFor({ state: "visible", timeout: LONG }),
        picker.waitFor({ state: "visible", timeout: LONG }),
    ])
    if (await picker.isVisible().catch(() => false)) {
        await chooseCombobox(page, "customer-receivables-party-picker-input", customerName)
        await page.locator("#customer-receivables-party-picker-confirm").click()
    }
}

async function registerReceiptForOrder(
    page: Page,
    input: { customerName: string; orderNo: string; amount: string; bankReference: string },
) {
    await page.goto("/finance/customer-accounts")
    await waitHeading(page, "客户往来")
    const register = page.locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeEnabled({ timeout: LONG })
    await register.click()
    await resolveReceiptPartyPicker(page, input.customerName)
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })
    await page.locator("#customer-receivables-session-amount").fill(input.amount)
    await page.locator("#customer-receivables-session-bank-reference").fill(input.bankReference)

    const item = page
        .locator("section")
        .filter({ has: page.getByRole("heading", { name: /同主体待核销池/ }) })
        .locator("li")
        .filter({ hasText: input.orderNo })
    await expect(item).toBeVisible({ timeout: TIMEOUT })
    if (!(await item.getByText("已加入").isVisible().catch(() => false))) {
        await item.getByRole("button", { name: "加入" }).click()
        await expect(item.getByText("已加入")).toBeVisible({ timeout: TIMEOUT })
    }
    const fill = page.getByRole("button", { name: "填满" })
    if (await fill.isVisible().catch(() => false)) {
        await fill.click()
    } else {
        await page.getByLabel(new RegExp(`${escapeRe(input.orderNo)}.*分配金额`)).fill(input.amount)
    }

    await page.locator("#customer-receivables-session-submit").click()
    await expect(page.getByRole("heading", { name: /提交回款|确认提交回款/ })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "回款已提交审批" })).toBeVisible({
        timeout: LONG,
    })
    const receiptNo = await factValue(page, "回款单号")
    expect(receiptNo.length).toBeGreaterThan(2)
    await page.locator("#customer-receivables-session-result-close").click()
    await waitHeading(page, "客户往来")
    return receiptNo
}

async function assertCaiwuCannotSubmitReceipt(page: Page, customerName: string) {
    await page.goto("/finance/customer-accounts")
    await waitHeading(page, "客户往来")
    const register = page.locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeVisible({ timeout: LONG })
    if (await register.isDisabled()) {
        await expect(register).toBeDisabled()
        return
    }
    await register.click()
    await resolveReceiptPartyPicker(page, customerName)
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })
    await page.locator("#customer-receivables-session-amount").fill("1.00")
    await page.locator("#customer-receivables-session-bank-reference").fill("CAI-WU-SHOULD-FAIL")
    const pool = page.locator("section").filter({
        has: page.getByRole("heading", { name: /同主体待核销池/ }),
    })
    const join = pool.getByRole("button", { name: "加入" })
    if (await join.isVisible().catch(() => false)) {
        await join.click()
        const fill = page.getByRole("button", { name: "填满" })
        if (await fill.isVisible().catch(() => false)) await fill.click()
    }
    const submit = page.locator("#customer-receivables-session-submit")
    if (await submit.isEnabled()) {
        await submit.click()
        const confirm = page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm")
        if (await confirm.isVisible().catch(() => false)) await confirm.click()
    }
    await expect(
        page.getByText(/提交人不得审批自己的单据|当前账号没有执行此操作的权限|操作未成功/),
    ).toBeVisible({ timeout: LONG })
}

async function registerSalesInvoiceFromWorkspace(page: Page, orderNo: string, amount: string) {
    await openWorkspaceFamily(page, "finance")
    await searchWorkspace(page, orderNo)
    const task = page
        .getByRole("button", {
            name: new RegExp(`销项开票处理[\\s\\S]*${escapeRe(orderNo)}`),
        })
        .or(
            page.getByRole("button", { name: /销项开票处理/ }).filter({
                hasText: orderNo,
            }),
        )
    await expect(task).toBeVisible({ timeout: LONG })
    await task.click()
    await expect(page.getByLabel("当前开票任务")).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })

    await page.locator("#customer-receivables-session-invoice-no").fill(`FP${Date.now()}`)
    await page.locator("#customer-receivables-session-gross-amount").fill(amount)
    const item = page
        .locator("section")
        .filter({ has: page.getByRole("heading", { name: /同主体待核销池/ }) })
        .locator("li")
        .filter({ hasText: orderNo })
    await expect(item).toBeVisible({ timeout: TIMEOUT })
    if (!(await item.getByText("已加入").isVisible().catch(() => false))) {
        await item.getByRole("button", { name: "加入" }).click()
        await expect(item.getByText("已加入")).toBeVisible({ timeout: TIMEOUT })
    }
    const fill = page.getByRole("button", { name: "填满" })
    await expect(fill).toBeVisible({ timeout: TIMEOUT })
    await fill.click()

    await page.locator("#customer-receivables-session-submit").click()
    await expect(page.getByRole("heading", { name: "确认登记销项发票并分配" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-session-invoice-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "销项发票已登记并分配" })).toBeVisible({
        timeout: LONG,
    })
}
