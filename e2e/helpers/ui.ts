import { expect, type Page } from "@playwright/test"

/** 流程断言默认超时，与 playwright.config expect.timeout 对齐。 */
export const UI_TIMEOUT = 20_000

/**
 * 等待 sonner/shadcn toast 标题出现，确认后关闭悬浮提示，
 * 避免提示遮挡后续按钮造成偶发点击失败。
 */
export async function expectToast(
    page: Page,
    title: string | RegExp,
): Promise<void> {
    const toast = page
        .locator('[data-slot="toast-title"]')
        .filter({ hasText: title })
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT })
    await dismissToasts(page)
}

/**
 * 关闭当前全部可关闭的悬浮提示。
 */
export async function dismissToasts(page: Page): Promise<void> {
    for (let i = 0; i < 5; i += 1) {
        const dismiss = page
            .locator('[data-slot="toast"]')
            .getByRole("button", { name: "关闭提示", includeHidden: true })
            .first()
        if (!(await dismiss.count())) return
        await dismiss.click({ timeout: 5_000 }).catch(() => undefined)
    }
}

/**
 * 确认已进入 W01 我的工作台。
 */
export async function expectWorkspace(page: Page): Promise<void> {
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}

const WORKSPACE_FAMILY_LABEL: Record<string, string> = {
    all: "全部",
    approval: "审批",
    procurement: "采购",
    fulfillment: "履约",
    finance: "财务",
    exception: "异常",
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
 * 工作台任务类型已改为下拉菜单。先打开菜单再点选项；已选中则跳过。
 */
export async function selectWorkspaceFamily(
    page: Page,
    family:
        | "approval"
        | "procurement"
        | "fulfillment"
        | "finance"
        | "exception"
        | "all"
        | string,
): Promise<void> {
    const wanted = WORKSPACE_FAMILY_LABEL[family] ?? family
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
            .locator(`#workspace-family-nav-${family}`)
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

    // 工作台筛选栏当前暂停展示，改走 URL family 过滤。
    const next = new URL(page.url())
    if (!next.pathname.startsWith("/workspace")) {
        next.pathname = "/workspace"
        next.search = ""
    }
    if (family === "all") next.searchParams.delete("family")
    else next.searchParams.set("family", family)
    const href = `${next.pathname}${next.search}`
    if (!page.url().endsWith(href) && !page.url().includes(`${href}&`)) {
        await page.goto(href)
    }
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}
