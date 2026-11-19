import { workPage } from "./workspace-page"
import { expect, type Locator, type Page } from "@playwright/test"

import { takeFreshWorkspace, WORKSPACE_TASKS_PATH } from "./login"

/**
 * 通用 UI 操作。spec 不要再复制 expectToast / chooseOption / pickCalendarDay /
 * openWorkspaceTask / approveCurrentDocument。
 *
 *   import {
 *     expectToast,
 *     chooseOption,
 *     expectComboboxValue,
 *     pickCalendarDay,
 *     openWorkspaceTask,
 *     selectWorkspaceFamily,
 *     approveCurrentDocument,
 *   } from "../helpers/ui"
 *
 * 生产 id / 文案：
 * - toast：`[data-slot="toast"]` / `[data-slot="toast-title"]`，关闭按钮 aria-label「关闭提示」
 * - combobox：字段 id 落在 input 上，选项 `#${id}-option-<value>`，`[data-slot="combobox-item"]`
 * - 日历：刚打开的 `[data-slot="popover-content"]` 内的 `[data-slot="calendar"]`，
 *   日期 `#${id}-calendar-month-YYYY-MM-01-day-YYYY-MM-DD`
 * - 待办页：表格「待办事项」、任务范围「待我处理 / 范围内待办」、独立详情「当前工作台任务」
 * - 审批：按钮「同意审批」/「通过」、对话框「确认通过」、`[id$="-decision-dialog-submit"]`
 */

/** 流程断言默认超时，与 playwright.config expect.timeout 对齐。 */
export const UI_TIMEOUT = 20_000

export type WorkspaceFamily =
    | "all"
    | "approval"
    | "procurement"
    | "fulfillment"
    | "finance"
    | "exception"
    | "全部"
    | "审批"
    | "采购"
    | "履约"
    | "财务"
    | "异常"
    | string

const WORKSPACE_FAMILY_KEYS: Record<string, string> = {
    全部: "all",
    审批: "approval",
    采购: "procurement",
    履约: "fulfillment",
    财务: "finance",
    异常: "exception",
}

const WORKSPACE_FAMILY_TYPES: Record<string, string[]> = {
    all: [],
    approval: ["DOCUMENT_APPROVAL", "APPROVAL_REWORK"],
    procurement: ["SALES_INQUIRY_PROCESSING", "SUPPLIER_PORTAL_REVIEW", "PROCUREMENT_ORDER_CREATION", "PURCHASE_PAYMENT_ARRANGEMENT", "PURCHASE_SOURCE_UPDATE"],
    fulfillment: ["FULFILLMENT_OPERATION", "CUSTOMER_ACCEPTANCE_REGISTRATION"],
    finance: ["PURCHASE_RETURN_REFUND", "PURCHASE_RETURN_INVOICE", "SUPPLIER_PAYMENT_EXECUTION", "SALES_INVOICE_EXECUTION", "SUPPLIER_SETTLEMENT_REVIEW"],
    exception: ["IMPORT_BUSINESS_CONFIRMATION", "INTEGRATION_RESULT_UNKNOWN", "BUSINESS_EXCEPTION"],
}

function workspaceFamilyTypes(family: string): string[] {
    const key = WORKSPACE_FAMILY_KEYS[family] ?? family
    const types = WORKSPACE_FAMILY_TYPES[key]
    if (!types) throw new Error(`未知待办类型组：${family}`)
    return types.map((type) => `task:${type}`)
}

function visibleToast(page: Page, title: string | RegExp): Locator {
    const root = workPage(page).locator('[data-slot="toast"]').filter({ hasText: title })
    const titled = workPage(page).locator('[data-slot="toast-title"]').filter({ hasText: title })
    return titled.or(root)
}

/**
 * 等待 toast 标题或 toast 根节点文本出现，确认后关闭，避免遮挡后续点击。
 * 兼容 `data-slot="toast-title"` 与只有根节点文本的情况。
 */
export async function expectToast(
    page: Page,
    title: string | RegExp,
): Promise<void> {
    await expect(visibleToast(page, title).first()).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    await dismissToasts(page)
}

/**
 * 关闭当前全部可关闭的悬浮提示。
 * 只处理可见的关闭按钮：已隐藏的 toast 不会挡点击。直接派发 click，不等可操作性：
 * 退场动画中或被弹窗遮罩盖住的 toast 真实点击会一直等到超时，而这里只需要它关掉。
 */
export async function dismissToasts(page: Page): Promise<void> {
    await page.mouse.move(8, 8).catch(() => undefined)
    for (let i = 0; i < 5; i += 1) {
        const dismiss = workPage(page)
            .locator('[data-slot="toast"]')
            .getByRole("button", { name: "关闭提示", includeHidden: true })
            .filter({ visible: true })
            .first()
        if (!(await dismiss.count())) return
        await dismiss.dispatchEvent("click", undefined, { timeout: 1_000 }).catch(() => undefined)
    }
}

function visibleComboboxPopup(page: Page, controlledId?: string | null): Locator {
    const opened = workPage(page).locator(
        '[data-slot="combobox-content"][data-open]:not([data-closed]):not([data-ending-style]):visible',
    )
    // aria-controls 指向 Popup 内的 Listbox。不要把上一个字段的退场浮层算进当前选择。
    return controlledId
        ? opened.filter({ has: workPage(page).locator(`[id=${JSON.stringify(controlledId)}]`) })
        : opened
}

function comboboxOption(
    popup: Locator,
    inputId: string | null,
    option: string | RegExp,
): Locator {
    const byRole = popup.getByRole("option", { name: option })
    const bySlot = popup.locator('[data-slot="combobox-item"]').filter({ hasText: option })
    if (!inputId) return byRole.or(bySlot).first()
    const byId = popup.locator(`[id^="${inputId}-option-"]`).filter({ hasText: option })
    return byId.or(byRole).or(bySlot).first()
}

/**
 * 下拉已收起且输入框显示选项文案，表示 TanStack Form 值已写入，而不是只键入了过滤词。
 */
export async function expectComboboxValue(
    page: Page,
    input: Locator,
    option: string,
): Promise<void> {
    try {
        await expect(input).toHaveAttribute("aria-expanded", "false", { timeout: 5_000 })
    } catch {
        const current = await input.inputValue().catch(() => "")
        throw new Error(
            `combobox 选项未写入 form：下拉仍打开，输入值为 ${JSON.stringify(current)}，期望 ${JSON.stringify(option)}`,
        )
    }
    await expect(input).not.toHaveValue("", { timeout: 5_000 })
    const current = await input.inputValue()
    // 客户选中后常显示「法定名称（简称）」；合同选中后常显示客户名而不是合同号。
    if (current.includes(option) || option.includes(current)) return
}

/**
 * 对 shadcn/Base UI OptionCombobox 选中一项。
 * `typed` 用于远程搜索或简称过滤（如负责人填 caigou，选项匹配 /采购/）。
 */
export async function chooseOption(
    page: Page,
    input: Locator,
    option: string | RegExp,
    typed?: string,
): Promise<void> {
    await expect(input).toBeVisible({ timeout: UI_TIMEOUT })
    const inputId = await input.getAttribute("id")
    await input.click()
    const query = typed ?? (typeof option === "string" ? option : "")
    if (query) {
        await input.fill(query)
    }

    // Base UI 默认 input 点击即打开。等所属 Listbox 挂载，避免加载期间误点 trigger 又关掉浮层。
    await expect(input).toHaveAttribute("aria-expanded", "true", { timeout: UI_TIMEOUT })
    await expect(input).toHaveAttribute("aria-controls", /.+/, { timeout: UI_TIMEOUT })
    const controlledId = await input.getAttribute("aria-controls")
    const popup = visibleComboboxPopup(page, controlledId)
    const ownedPopup = workPage(page).locator('[data-slot="combobox-content"]').filter({
        has: workPage(page).locator(`[id=${JSON.stringify(controlledId)}]`),
    })

    const listed = comboboxOption(popup, inputId, option)
    try {
        await expect(listed).toBeVisible({ timeout: UI_TIMEOUT })
    } catch {
        const empty = (await popup
            .locator('[data-slot="combobox-empty"]')
            .allTextContents()).join(" ").trim()
        throw new Error(
            `未找到 combobox 选项 ${String(option)}${empty ? `（${empty}）` : ""}`,
        )
    }
    await listed.click()
    // Base UI 选中后仍会播放退场动画。等待提交状态，不再点击正在消失的选项：
    // 退场期间 isVisible 可能为 true，而下一次 click/innerText 会空等默认 actionTimeout。
    if (typeof option === "string") {
        await expectComboboxValue(page, input, option)
        await expect(ownedPopup).toBeHidden({ timeout: 5_000 })
        return
    }
    try {
        await expect(input).toHaveAttribute("aria-expanded", "false", { timeout: 5_000 })
        await expect(input).not.toHaveValue("", { timeout: 5_000 })
    } catch {
        throw new Error(`combobox 选项未提交：下拉仍打开（${String(option)}）`)
    }
    await expect(ownedPopup).toBeHidden({ timeout: 5_000 })
}

const MONTH_NAMES_EN = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
] as const

const MONTH_ABBR_EN = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
] as const

/**
 * 打开 DatePicker 并点选 `YYYY-MM-DD`。
 * 优先 `#${triggerId}-calendar-month-*-day-${isoDate}`，否则翻月后再点子串日期按钮。
 */
export async function pickCalendarDay(
    page: Page,
    trigger: Locator,
    isoDate: string,
): Promise<void> {
    await expect(trigger).toBeVisible({ timeout: UI_TIMEOUT })
    const fieldId = await trigger.getAttribute("id")
    await trigger.click()

    const opened = workPage(page)
        .locator('[data-slot="popover-content"]:visible')
        .filter({ has: workPage(page).locator('[data-slot="calendar"]') })
    // 同一页可能还有未关掉的日历，或外层浮层套着日期浮层。优先当前字段、且自身不再嵌套浮层的那一个。
    const owned = fieldId
        ? opened.filter({ has: workPage(page).locator(`[id^="${fieldId}-calendar"]`) })
        : opened
    const popover = owned
        .filter({ hasNot: workPage(page).locator('[data-slot="popover-content"]') })
        .last()
    // 范围日历可能并排两个月，各有一个 calendar。先收窄到浮层再 first，避免 strict mode。
    const calendar = popover.locator('[data-slot="calendar"]').first()
    await expect(calendar).toBeVisible({ timeout: UI_TIMEOUT })

    const selectDay = async (button: Locator): Promise<void> => {
        await button.click()
        await expect(trigger).toHaveAttribute("aria-expanded", "false")
        await expect(popover).toBeHidden()
    }
    const dayButtonsById = fieldId
        ? popover.locator(`[id^="${fieldId}-calendar"][id$="-day-${isoDate}"]`)
        : popover.locator(`[id$="-day-${isoDate}"]`)
    const clickIsoDay = async (): Promise<boolean> => {
        const total = await dayButtonsById.count()
        let outsideFallback: Locator | undefined
        for (let index = 0; index < total; index += 1) {
            const button = dayButtonsById.nth(index)
            if (!(await button.isVisible().catch(() => false))) continue
            if ((await button.getAttribute("aria-disabled")) === "true") continue
            if ((await button.getAttribute("data-outside")) === "true") {
                outsideFallback ??= button
                continue
            }
            await selectDay(button)
            return true
        }
        if (!outsideFallback) return false
        await selectDay(outsideFallback)
        return true
    }

    let nextMonth = popover
        .locator('[id$="-next-month"]')
        .or(
            popover.getByRole("button", {
                name: /next month|go to the next month|下个月|下一月/i,
            }),
        )
    if (fieldId) {
        nextMonth = nextMonth
            .or(popover.locator(`#${fieldId}-calendar-next-month`))
            .or(popover.locator(`#${fieldId}-next-month`))
    }
    nextMonth = nextMonth.first()

    for (let i = 0; i < 18; i += 1) {
        if (await clickIsoDay()) return
        const target = new Date(`${isoDate}T00:00:00`)
        const caption = await popover.innerText()
        const monthTokens = [
            `${target.getMonth() + 1}月`,
            MONTH_NAMES_EN[target.getMonth()]!,
            MONTH_ABBR_EN[target.getMonth()]!,
        ]
        const yearOk = caption.includes(String(target.getFullYear()))
        const monthOk = monthTokens.some((token) => caption.includes(token))
        if (yearOk && monthOk) break
        if (await nextMonth.count()) {
            await nextMonth.click()
            continue
        }
        break
    }

    if (await clickIsoDay()) return

    const day = String(new Date(`${isoDate}T00:00:00`).getDate())
    const dayButtons = popover.getByRole("button", { name: day })
    const total = await dayButtons.count()
    for (let i = 0; i < total; i += 1) {
        const button = dayButtons.nth(i)
        const disabled = await button.getAttribute("aria-disabled")
        const outside = await button.getAttribute("data-outside")
        if (disabled === "true" || outside === "true") continue
        await selectDay(button)
        return
    }
    if (total === 0) {
        throw new Error(`日历未找到日期 ${isoDate}`)
    }
    await selectDay(dayButtons.first())
}

function escapeRe(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

/**
 * 确认已进入当前待办队列。
 */
export async function expectWorkspace(page: Page): Promise<void> {
    await expect(workPage(page).getByRole("heading", { name: /^(待我处理|范围内待办)$/ })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}

/**
 * 已生效销售单金额与回款/开票状态在侧栏「销售单摘要」，
 * 页头 identityOnly 不再渲染金额条。
 */
export function salesOrderAmountSummary(page: Page) {
    return workPage(page)
        .locator('[aria-label="销售单摘要"]')
        .or(workPage(page).locator('[aria-label="销售单金额摘要"]'))
        .or(workPage(page).locator('[data-slot="metric-strip"]'))
        .first()
}

/**
 * 可有可无的中间步骤（如二次确认框）与后续结果谁先出现。
 * `optional` 先可见返回 true；`settled`（结果元素或请求 Promise）先到返回 false；都没到则 timeout 后返回 false。
 * 不要用 `isVisible({ timeout })` 做这件事：Playwright 忽略该 timeout，立即返回。
 */
export async function optionalStepVisible(
    optional: Locator,
    settled: Locator | Promise<unknown>,
    timeout = UI_TIMEOUT,
): Promise<boolean> {
    const shown = optional
        .first()
        .waitFor({ state: "visible", timeout })
        .then(
            () => true,
            () => false,
        )
    const done = (
        settled instanceof Promise
            ? settled
            : settled.first().waitFor({ state: "visible", timeout })
    ).then(
        () => false,
        () => false,
    )
    return Promise.race([shown, done])
}

/**
 * openWorkspaceTask 已按 family、任务类型与来源提示选定任务。
 * 当前任务按履约类型显示登记入口，使用同一 workItem 的原生 id 打开对应表单。
 */
export async function openFulfillmentWorkspaceForm(page: Page) {
    const opened = workPage(page)
        .getByRole("dialog")
        .filter({ has: workPage(page).locator('[aria-label="当前履约任务"]') })
        .filter({ visible: true })
    const task = workPage(page).getByRole("region", {
        name: "当前工作台任务",
        exact: true,
    })
    const trigger = task
        .getByRole("button", {
            name: /^(登记发货|登记入库|登记电子交付|登记服务完成|处理履约)$/,
        })
        .and(task.locator('button[id^="workspace-fulfillment-process-"]'))
    const inline = task.getByRole("region", { name: "当前履约任务", exact: true }).filter({
        has: workPage(page).locator('[aria-label$="表单"]:visible, #fulfillment-packing-form:visible'),
        visible: true,
    })
    await expect(opened.or(inline).or(trigger).filter({ visible: true }).first()).toBeVisible({ timeout: UI_TIMEOUT })
    const surface = opened.or(inline)
    if (await surface.count()) {
        await expectFulfillmentForm(surface)
        return surface
    }
    const triggerId = await trigger.getAttribute("id")
    expect(triggerId).toMatch(/^workspace-fulfillment-process-.+/)
    const taskId = triggerId!.slice("workspace-fulfillment-process-".length)
    const dialog = workPage(page).locator(
        `[role="dialog"][id=${JSON.stringify(`workspace-fulfillment-process-dialog-${taskId}`)}]`,
    )
    const action = (await trigger.innerText()).trim()
    const formLabels: Record<string, string[]> = {
        登记发货: ["按地址登记发货包裹", "供应商直发表单", "公司仓发表单"],
        登记入库: ["入库表单"],
        登记电子交付: ["电子交付表单"],
        登记服务完成: ["线下服务表单"],
    }
    // 任务详情会随轮询重绘；每次重新解析同一 workItem 的按钮并等待它的对话框。
    await expect(async () => {
        if (await dialog.isVisible()) return
        const button = task.locator(`[id=${JSON.stringify(triggerId)}]`)
        await button.scrollIntoViewIfNeeded({ timeout: 5_000 })
        await button.click({ timeout: 5_000 })
        await expect(dialog).toBeVisible({ timeout: 5_000 })
    }).toPass({ timeout: UI_TIMEOUT })
    await expectFulfillmentForm(dialog, formLabels[action])
    return dialog
}

async function expectFulfillmentForm(
    dialog: Locator,
    labels = ["按地址登记发货包裹", "供应商直发表单", "公司仓发表单", "入库表单", "电子交付表单", "线下服务表单"],
) {
    const form = dialog.locator(
        labels.map((label) => `[aria-label=${JSON.stringify(label)}]`).join(", "),
    )
    await expect(form).toHaveCount(1, { timeout: UI_TIMEOUT })
    await expect(form).toBeVisible({ timeout: UI_TIMEOUT })
}

/**
 * 详情页头部单号已改为「单号：XS…」纯文本，不再使用 span.num。
 */
export async function readHeaderDocumentNumber(
    page: Page,
    numberLabel = "单号",
): Promise<string> {
    const node = workPage(page)
        .locator("header")
        .getByText(new RegExp(`^${numberLabel}[:：]`))
        .first()
    await expect(node).toBeVisible({ timeout: UI_TIMEOUT })
    const text = (await node.innerText()).trim()
    const number = text
        .replace(new RegExp(`^${numberLabel}[:：]\\s*`), "")
        .split(/\s+/)[0]
        ?.trim()
    expect(number && number.length > 2, `未能读取${numberLabel}: ${text}`).toBeTruthy()
    return number!
}

/** 为旧流程使用的类型组设置当前待办页的正式事项类型筛选。 */
export async function selectWorkspaceFamily(page: Page, family: WorkspaceFamily): Promise<void> {
    const current = new URL(page.url())
    const queue = current.pathname === WORKSPACE_TASKS_PATH
    const search = queue ? current.searchParams : new URLSearchParams()
    const filters = JSON.parse(search.get("filters") ?? "{}") as Record<string, unknown>
    const types = workspaceFamilyTypes(family)
    if (types.length) filters.item_types = types
    else delete filters.item_types
    if (Object.keys(filters).length) search.set("filters", JSON.stringify(filters))
    else search.delete("filters")
    search.delete("page")
    const query = search.toString()
    const href = `${WORKSPACE_TASKS_PATH}${query ? `?${query}` : ""}`
    if (!queue || `${current.pathname}${current.search}` !== href) await page.goto(href)
    await expectWorkspace(page)
    await expectWorkspaceQueue(page)
}

export function workspaceTaskRows(page: Page): Locator {
    return workPage(page).locator('tr[id^="workspace-task-table-row-"][data-row-id][tabindex="0"]')
}

export async function expectWorkspaceQueue(page: Page): Promise<void> {
    const queue = workPage(page).getByRole("region", { name: "任务队列", exact: true })
    await expect(queue).toBeVisible({ timeout: UI_TIMEOUT })
    await expect(workPage(page).locator("#workspace-home-refresh")).toBeEnabled({ timeout: UI_TIMEOUT })
}

export function workspaceTaskId(page: Page): string | null {
    const match = /^\/workspace\/(?:tasks|approvals)\/([^/]+)$/.exec(new URL(page.url()).pathname)
    return match ? decodeURIComponent(match[1]!) : null
}

export async function selectWorkspaceScope(page: Page, scope: "inbox" | "managed"): Promise<void> {
    const select = workPage(page).locator("#workspace-task-scope")
    await select.click()
    await workPage(page).locator(`#workspace-task-scope-option-${scope}`).click()
    await expect(workPage(page).getByRole("heading", { name: scope === "inbox" ? "待我处理" : "范围内待办", exact: true })).toBeVisible()
    await expectWorkspaceQueue(page)
}

export function matchingWorkspaceTaskRows(page: Page, typeLabel: string | RegExp, hint?: string): Locator {
    const rows = workspaceTaskRows(page)
    const label = typeof typeLabel === "string" ? new RegExp(escapeRe(typeLabel)) : typeLabel
    const typed = rows.and(workPage(page).getByRole("row", { name: label })).or(rows.filter({ hasText: label }))
    if (!hint) return typed
    const source = new RegExp(escapeRe(hint))
    return typed.and(rows.and(workPage(page).getByRole("row", { name: source })).or(rows.filter({ hasText: source })))
}

/** 验证当前供给任务实际可见的桌面或紧凑摘要，避免同时读取隐藏布局。 */
export async function expectSourcingCounts(
    page: Page,
    purchaseOrders?: number,
    stockReservations?: number,
): Promise<void> {
    const footer = workPage(page).getByRole("region", { name: "当前工作台任务" })
        .locator('[data-slot="workspace-task-footer"]:visible')
    await expect(footer).toHaveCount(1, { timeout: UI_TIMEOUT })
    const totals = footer.locator("dl:visible").or(
        footer.locator("p:visible").filter({ hasText: /^将创建采购单\s/ }),
    )
    await expect(totals).toHaveCount(1, { timeout: UI_TIMEOUT })
    await expect(totals).toBeVisible({ timeout: UI_TIMEOUT })
    if (purchaseOrders !== undefined) {
        await expect(totals).toContainText(new RegExp(`将创建采购单\\s*${purchaseOrders}\\s*张`))
    }
    if (stockReservations !== undefined) {
        await expect(totals).toContainText(new RegExp(`(?:将建立)?库存预留\\s*${stockReservations}\\s*条`))
    }
}

/** 从最新的待办队列打开匹配事项，并核对独立详情地址上的任务 ID。 */
export async function openWorkspaceTask(
    page: Page,
    typeLabel: string | RegExp,
    hint?: string,
    family?: WorkspaceFamily,
): Promise<void> {
    if (!takeFreshWorkspace(page)) await page.goto(WORKSPACE_TASKS_PATH)
    if (family) await selectWorkspaceFamily(page, family)
    else await expectWorkspaceQueue(page)

    const task = matchingWorkspaceTaskRows(page, typeLabel, hint).first()
    if (!(await task.isVisible())) {
        const scope = workPage(page).locator("#workspace-task-scope")
        if (await scope.isVisible()) await selectWorkspaceScope(page, "managed")
    }
    try {
        await expect(task).toBeVisible({ timeout: UI_TIMEOUT })
    } catch {
        const rows = await workspaceTaskRows(page).evaluateAll((nodes) =>
            nodes.map((node) => node.getAttribute("aria-label") || node.textContent || ""),
        )
        throw new Error(`待办中找不到任务：${String(typeLabel)}${hint ? ` / ${hint}` : ""}\n当前事项：${rows.join(" | ") || "（空）"}`)
    }
    const id = await task.getAttribute("data-row-id")
    expect(id, "待办行须具有正式任务身份").toBeTruthy()
    await openWorkspaceTaskRow(page, id!)
}

/** 任务材料与任务身份独立；已知工作项 ID 时直接定位当前队列行。 */
export async function openWorkspaceTaskRow(page: Page, id: string): Promise<void> {
    const selected = workspaceTaskRows(page).and(workPage(page).locator(`[data-row-id=${JSON.stringify(id)}]`))
    await expect(selected).toBeVisible({ timeout: UI_TIMEOUT })
    await selected.press("Enter")
    await expect.poll(() => workspaceTaskId(page), { timeout: UI_TIMEOUT }).toBe(id)
    await expect(workPage(page).getByRole("region", { name: "当前工作台任务" })).toBeVisible({ timeout: UI_TIMEOUT })
}

/**
 * 工作台或对象中心审批通过。按钮文案：工作台「同意审批」，其它页「通过」。
 */
export async function approveCurrentDocument(page: Page): Promise<void> {
    const approve = workPage(page)
        .locator('[id$="-approve"]')
        .filter({ hasText: /通过|同意审批/ })
        .or(workPage(page).getByRole("button", { name: /^(通过|同意审批)$/ }))
        .first()
    await expect(approve).toBeVisible({ timeout: UI_TIMEOUT })
    await approve.click()
    const dialog = workPage(page).getByRole("dialog", { name: "确认通过" })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    const confirm = dialog
        .locator('[id$="-decision-dialog-submit"]')
        .or(dialog.getByRole("button", { name: "确认通过" }))
        .first()
    await dismissToasts(page)
    const decided = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/approval-decisions",
        { timeout: 60_000 },
    )
    await confirm.click()
    const response = await decided
    const body = await response.text()
    const diagnostics = `审批决定失败（HTTP ${response.status()}）: ${body}`
    expect(response.ok(), diagnostics).toBe(true)
    const result = JSON.parse(body) as { success?: boolean }
    expect(result.success, diagnostics).toBe(true)
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT })
}
