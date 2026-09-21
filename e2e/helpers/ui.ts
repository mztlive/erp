import { expect, type Locator, type Page } from "@playwright/test"

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
 * - 日历：`[data-slot="calendar"]`，日期 `#${id}-calendar-month-YYYY-MM-01-day-YYYY-MM-DD`
 * - 工作台：heading「我的工作台」、`#workspace-family-filter-trigger`、
 *   `#workspace-family-nav-{all|approval|procurement|fulfillment|finance|exception}`、
 *   list「待办列表」、region「当前工作台任务」
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

const WORKSPACE_FAMILY_LABEL: Record<string, string> = {
    all: "全部",
    approval: "审批",
    procurement: "采购",
    fulfillment: "履约",
    finance: "财务",
    exception: "异常",
    全部: "全部",
    审批: "审批",
    采购: "采购",
    履约: "履约",
    财务: "财务",
    异常: "异常",
}

const WORKSPACE_FAMILY_KEY: Record<string, string> = {
    all: "all",
    approval: "approval",
    procurement: "procurement",
    fulfillment: "fulfillment",
    finance: "finance",
    exception: "exception",
    全部: "all",
    审批: "approval",
    采购: "procurement",
    履约: "fulfillment",
    财务: "finance",
    异常: "exception",
}

/** 把中文任务类型或英文 family 收成 URL / `#workspace-family-nav-*` 用的键。 */
export function workspaceFamilyKey(family: string): string {
    return WORKSPACE_FAMILY_KEY[family] ?? family
}

function workspaceFamilyLabel(family: string): string {
    return WORKSPACE_FAMILY_LABEL[family] ?? family
}

function visibleToast(page: Page, title: string | RegExp): Locator {
    const root = page.locator('[data-slot="toast"]').filter({ hasText: title })
    const titled = page.locator('[data-slot="toast-title"]').filter({ hasText: title })
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
 */
export async function dismissToasts(page: Page): Promise<void> {
    await page.mouse.move(8, 8).catch(() => undefined)
    for (let i = 0; i < 5; i += 1) {
        const dismiss = page
            .locator('[data-slot="toast"]')
            .getByRole("button", { name: "关闭提示", includeHidden: true })
            .first()
        if (!(await dismiss.count())) return
        await dismiss.click({ timeout: 5_000 }).catch(() => undefined)
    }
}

function visibleComboboxPopup(page: Page): Locator {
    return page.locator('[data-slot="combobox-content"]:visible')
}

function comboboxOption(
    page: Page,
    inputId: string | null,
    option: string | RegExp,
): Locator {
    const byRole = page.getByRole("option", { name: option })
    const bySlot = page.locator('[data-slot="combobox-item"]').filter({ hasText: option })
    if (!inputId) return byRole.or(bySlot).first()
    const byId = page.locator(`[id^="${inputId}-option-"]`).filter({ hasText: option })
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
    const popup = visibleComboboxPopup(page)
    try {
        await expect(popup).toBeHidden({ timeout: 5_000 })
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
        await input.fill("")
        await input.fill(query)
    }

    const popup = visibleComboboxPopup(page)
    if (!(await popup.isVisible().catch(() => false))) {
        if (inputId) {
            const trigger = page.locator(`#${inputId}-trigger`)
            if (await trigger.count()) await trigger.click()
        }
    }

    const listed = comboboxOption(page, inputId, option)
    try {
        await expect(listed).toBeVisible({ timeout: UI_TIMEOUT })
    } catch {
        const empty = (
            await page.locator('[data-slot="combobox-empty"]').innerText().catch(() => "")
        ).trim()
        throw new Error(
            `未找到 combobox 选项 ${String(option)}${empty ? `（${empty}）` : ""}`,
        )
    }
    await listed.click()
    if (await popup.isVisible().catch(() => false)) {
        await listed.click({ force: true }).catch(() => undefined)
    }
    if (await popup.isVisible().catch(() => false)) {
        await page.keyboard.press("Enter")
    }

    if (typeof option === "string") {
        await expectComboboxValue(page, input, option)
        return
    }
    try {
        await expect(popup).toBeHidden({ timeout: 5_000 })
    } catch {
        throw new Error(`combobox 选项未提交：下拉仍打开（${String(option)}）`)
    }
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
    await trigger.click()
    const calendar = page.locator('[data-slot="calendar"]:visible')
    await expect(calendar).toBeVisible({ timeout: UI_TIMEOUT })

    const dayById = calendar.locator(`[id$="-day-${isoDate}"]`).first()
    const fieldId = await trigger.getAttribute("id")
    let nextMonth = calendar
        .locator('[id$="-next-month"]')
        .or(
            calendar.getByRole("button", {
                name: /next month|go to the next month|下个月|下一月/i,
            }),
        )
    if (fieldId) {
        nextMonth = nextMonth
            .or(page.locator(`#${fieldId}-calendar-next-month`))
            .or(page.locator(`#${fieldId}-next-month`))
    }
    nextMonth = nextMonth.first()

    for (let i = 0; i < 18; i += 1) {
        if (await dayById.isVisible().catch(() => false)) {
            const disabled = await dayById.getAttribute("aria-disabled")
            if (disabled !== "true") {
                await dayById.click()
                return
            }
        }
        const target = new Date(`${isoDate}T00:00:00`)
        const caption = await calendar.innerText()
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

    if (await dayById.isVisible().catch(() => false)) {
        await dayById.click()
        return
    }

    const day = String(new Date(`${isoDate}T00:00:00`).getDate())
    const dayButtons = calendar.getByRole("button", { name: day })
    const total = await dayButtons.count()
    for (let i = 0; i < total; i += 1) {
        const button = dayButtons.nth(i)
        const disabled = await button.getAttribute("aria-disabled")
        const outside = await button.getAttribute("data-outside")
        if (disabled === "true" || outside === "true") continue
        await button.click()
        return
    }
    if (total === 0) {
        throw new Error(`日历未找到日期 ${isoDate}`)
    }
    await dayButtons.first().click()
}

function escapeRe(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

function labelSource(label: string | RegExp): string {
    return typeof label === "string" ? label : label.source
}

/**
 * 确认已进入 W01 我的工作台。
 */
export async function expectWorkspace(page: Page): Promise<void> {
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}

/**
 * 已生效销售单金额与回款/开票状态在侧栏「销售单摘要」，
 * 页头 identityOnly 不再渲染金额条。
 */
export function salesOrderAmountSummary(page: Page) {
    return page
        .locator('[aria-label="销售单摘要"]')
        .or(page.locator('[aria-label="销售单金额摘要"]'))
        .or(page.locator('[data-slot="metric-strip"]'))
        .first()
}

/** 金额摘要中带状态徽章的指标，例如「已回款 / 未收」。 */
export function salesOrderMetric(page: Page, metricLabel: string) {
    return salesOrderAmountSummary(page).locator("span").filter({ hasText: metricLabel }).first()
}

/**
 * 工作台履约任务先展示摘要，点「处理履约」后才打开表单对话框。
 */
export async function openFulfillmentWorkspaceForm(page: Page) {
    const dialog = page.getByRole("dialog", { name: "处理履约" })
    try {
        await expect(dialog).toBeVisible({ timeout: 2_000 })
        return dialog
    } catch {
        // 摘要页需要先点「处理履约」才打开表单对话框。
    }
    const trigger = page.getByRole("button", { name: "处理履约" })
    await expect(trigger).toBeVisible({ timeout: UI_TIMEOUT })
    await trigger.click({ force: true })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    return dialog
}

/**
 * 履约表单等把 aria-label 标在普通 section/div 上，getByLabel 匹配不到。
 */
export function labeledRegion(page: Page, name: string) {
    return page.locator(`[aria-label="${name}"]`)
}

/** 对话框确认按钮常因 toast/动画抖动点不中。 */
export async function forceClick(locator: { click: (opts?: { force?: boolean; timeout?: number }) => Promise<void> }) {
    await locator.click({ force: true, timeout: UI_TIMEOUT })
}

/**
 * 详情页头部单号已改为「单号：XS…」纯文本，不再使用 span.num。
 */
export async function readHeaderDocumentNumber(
    page: Page,
    numberLabel = "单号",
): Promise<string> {
    const node = page
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

/**
 * 工作台任务类型下拉。`family` 可以是 approval 或「审批」。
 * 生产 id：`#workspace-family-filter-trigger`、`#workspace-family-nav-{key}`。
 */
export async function selectWorkspaceFamily(
    page: Page,
    family: WorkspaceFamily,
): Promise<void> {
    const key = workspaceFamilyKey(family)
    const wanted = workspaceFamilyLabel(family)
    const trigger = page
        .locator("#workspace-family-filter-trigger")
        .or(page.getByRole("button", { name: /任务类型：/ }))
        .first()
    if (await trigger.isVisible().catch(() => false)) {
        const current =
            (await trigger.getAttribute("aria-label")) ||
            (await trigger.innerText())
        if (current.includes(wanted) && current.includes("任务类型")) return
        await trigger.click()
        const item = page
            .locator(`#workspace-family-nav-${key}`)
            .or(
                page.getByRole("menuitemradio", {
                    name: new RegExp(`^${wanted}`),
                }),
            )
            .first()
        await expect(item).toBeVisible({ timeout: UI_TIMEOUT })
        await item.click()
        await expect(trigger).toContainText(wanted, { timeout: UI_TIMEOUT })
        return
    }

    const next = new URL(page.url())
    if (!next.pathname.startsWith("/workspace")) {
        next.pathname = "/workspace"
        next.search = ""
    }
    if (key === "all") next.searchParams.delete("family")
    else next.searchParams.set("family", key)
    const href = `${next.pathname}${next.search}`
    if (!page.url().endsWith(href) && !page.url().includes(`${href}&`)) {
        await page.goto(href)
    }
    await expectWorkspace(page)
}

function workspaceTaskButtons(page: Page): Locator {
    return page
        .getByRole("list", { name: "待办列表" })
        .getByRole("button")
        .or(page.getByRole("button", { name: /审批|分配|履约|付款|开票|调整/ }))
}

function workspaceTaskLocator(
    page: Page,
    typeLabel: string | RegExp,
    hint?: string,
): Locator {
    const buttons = workspaceTaskButtons(page)
    const label = `(?:${labelSource(typeLabel)})`
    return hint
        ? buttons
              .filter({
                  hasText: new RegExp(
                      `${label}[\\s\\S]*${escapeRe(hint)}|${escapeRe(hint)}[\\s\\S]*${label}`,
                  ),
              })
              .or(
                  page.getByRole("button", {
                      name: new RegExp(
                          `${label}[\\s\\S]*${escapeRe(hint)}|${escapeRe(hint)}[\\s\\S]*${label}`,
                      ),
                  }),
              )
              .first()
        : buttons
              .filter({ hasText: new RegExp(label) })
              .or(page.getByRole("button", { name: new RegExp(label) }))
              .first()
}

/**
 * 打开 W01 待办。不要往 `#workspace-queue-toolbar-search-input` 填单号/往来方，
 * 后端搜索不匹配这些字段，会把列表滤空。
 *
 * `typeLabel` 可以是「销售单审批」或 `待供给分配|供给分配` 这种正则源。
 * `hint` 匹配 aria-label（单号）或可见文本（客户名）。
 */
export async function openWorkspaceTask(
    page: Page,
    typeLabel: string | RegExp,
    hint?: string,
    family?: WorkspaceFamily,
): Promise<void> {
    const key = family ? workspaceFamilyKey(family) : undefined
    const href = !key || key === "all" ? "/workspace" : `/workspace?family=${key}`
    await page.goto(href)
    await expectWorkspace(page)
    if (family) await selectWorkspaceFamily(page, family)

    const list = page.getByRole("list", { name: "待办列表" })
    const empty = page.getByText(
        /当前没有待处理事项|当前筛选没有待办|范围内没有待办/,
    )
    await expect(list.or(empty).first()).toBeVisible({ timeout: UI_TIMEOUT })

    const union = workspaceTaskLocator(page, typeLabel, hint)
    try {
        await expect(union).toBeVisible({ timeout: UI_TIMEOUT })
        await union.click()
    } catch {
        const labels = await workspaceTaskButtons(page)
            .evaluateAll((nodes) =>
                nodes.map((node) => node.getAttribute("aria-label") || node.textContent || ""),
            )
            .catch(() => [] as string[])
        const emptyText = (await empty.first().innerText().catch(() => "")).trim()
        throw new Error(
            `工作台未找到任务: ${String(typeLabel)}${hint ? ` / ${hint}` : ""}\n现有: ${labels.filter(Boolean).join(" | ") || emptyText || "（空）"}`,
        )
    }
    await expect(page.getByRole("region", { name: "当前工作台任务" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}

/**
 * 工作台或对象中心审批通过。按钮文案：工作台「同意审批」，其它页「通过」。
 */
export async function approveCurrentDocument(page: Page): Promise<void> {
    const approve = page
        .locator('[id$="-approve"]')
        .filter({ hasText: /通过|同意审批/ })
        .or(page.getByRole("button", { name: /^(通过|同意审批)$/ }))
        .first()
    await expect(approve).toBeVisible({ timeout: UI_TIMEOUT })
    await approve.click()
    const dialog = page.getByRole("dialog", { name: "确认通过" })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    const confirm = dialog
        .locator('[id$="-decision-dialog-submit"]')
        .or(dialog.getByRole("button", { name: "确认通过" }))
        .first()
    await confirm.click()
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT })
}
