import { Locator, Page, expect } from "@playwright/test"

/**
 * 通用 UI 工具：导航、按钮、表单、表格、弹窗与 toast 断言。
 * 各流程 spec 可自行扩展（保持 helper 通用，不放业务专有逻辑）。
 */

/** 跳转并等待页面稳定（SPA 路由）。 */
export async function gotoPage(page: Page, path: string): Promise<void> {
    await page.goto(path)
    await expect(page).toHaveURL(new RegExp(path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")))
}

/** 点击页面中可见的按钮（按可访问名称）。 */
export async function clickButton(page: Page, name: string | RegExp): Promise<void> {
    await page.getByRole("button", { name }).first().click()
}

/** 在新建采购单页预览并确认创建草稿。 */
export async function completePurchaseOrderCreate(page: Page): Promise<void> {
    await expect(page.getByRole("heading", { name: "新建采购单" })).toBeVisible({
        timeout: 20_000,
    })
    await page.getByTestId("purchase-create-preview").click()
    await expect(page.getByText("预览采购单").first()).toBeVisible({
        timeout: 10_000,
    })
    await page.getByTestId("purchase-create-from-basis").click()
    await page.getByTestId("purchase-create-confirm").click()
}

/** 点击弹窗/Dialog 内的按钮。 */
export async function clickDialogButton(page: Page, name: string | RegExp): Promise<void> {
    const dialog = page.locator('[role="dialog"], [data-slot="dialog"]').last()
    await dialog.getByRole("button", { name }).first().click()
}

/** 按 label 文本填充输入框（shadcn Field 的 label 与 input 关联）。 */
export async function fillByLabel(page: Page, label: string, value: string): Promise<void> {
    // 对话框可见时限定在对话框内，避免命中背后列表页表头的同名 aria-label；
    // 排除日历弹层（日期选择 Popover 也带 role=dialog，内部没有业务输入字段）
    const dialog = page
        .locator('[role="dialog"], [role="alertdialog"], [data-slot="dialog"]')
        .filter({ hasNot: page.locator('[data-slot="calendar"]') })
        .last()
    const dialogVisible = await dialog.isVisible().catch(() => false)
    const scope = dialogVisible ? dialog : page
    await scope.getByLabel(label, { exact: false }).fill(value)
}

/** 按 placeholder 填充输入框。 */
export async function fillByPlaceholder(page: Page, placeholder: string, value: string): Promise<void> {
    await page.getByPlaceholder(placeholder).fill(value)
}

/** 断言页面出现指定 toast/提示（成功或错误）。 */
export async function expectToast(page: Page, text: string | RegExp): Promise<void> {
    await expect(page.getByText(text).first()).toBeVisible({ timeout: 20_000 })
}

/** 等待 toast 消失（用于连续操作的提示间隔）。 */
export async function dismissToast(page: Page): Promise<void> {
    const close = page.locator('[data-slot="toast"] button, [role="status"] button').first()
    if (await close.isVisible().catch(() => false)) {
        await close.click().catch(() => {})
    }
}

/**
 * 打开新建/创建弹窗：点击列表页的创建按钮并等待 dialog 出现。
 */
export async function openCreateDialog(page: Page, buttonName: string | RegExp): Promise<Locator> {
    await clickButton(page, buttonName)
    const dialog = page.locator('[role="dialog"]').last()
    await expect(dialog).toBeVisible({ timeout: 20_000 })
    return dialog
}

/** 等待表格出现指定行文本（跨列搜索）。 */
export async function expectTableRow(
    page: Page,
    text: string,
    options: { timeout?: number; table?: Locator } = {},
): Promise<Locator> {
    const table = options.table ?? page.locator("table").first()
    const row = table.getByRole("row").filter({ hasText: text }).first()
    await expect(row).toBeVisible({ timeout: options.timeout ?? 20_000 })
    return row
}

/** 点击表格中某行的操作按钮（行内按钮，按名称）。 */
export async function clickRowAction(page: Page, rowText: string, buttonName: string): Promise<void> {
    const row = page.locator("table").first().getByRole("row").filter({ hasText: rowText }).first()
    await row.getByRole("button", { name: buttonName }).first().click()
}

/**
 * 通用下拉/组合框选择：点击触发按钮后选择可见选项。
 * combobox 触发元素通常带 aria-haspopup 或 role="combobox"。
 */
export async function pickOption(page: Page, trigger: Locator, optionText: string): Promise<void> {
    await trigger.click()
    const listbox = page.locator('[role="listbox"], [data-slot="popover"]').last()
    await expect(listbox).toBeVisible({ timeout: 10_000 })
    await listbox.getByText(optionText, { exact: false }).first().click()
}

/** 提交表单：点击表单内的提交按钮。 */
export async function submitForm(page: Page, buttonName: string | RegExp): Promise<void> {
    await page.locator("form").last().getByRole("button", { name: buttonName }).first().click()
}

/** 通用弹窗提交（保存/确认/提交），点击后等待弹窗关闭。 */
export async function confirmDialog(page: Page, buttonName: string | RegExp): Promise<void> {
    const dialog = page.locator('[role="dialog"]').last()
    await dialog.getByRole("button", { name: buttonName }).first().click()
    await expect(dialog).not.toBeVisible({ timeout: 20_000 }).catch(() => {})
}
