import { expect, type Page } from "@playwright/test"

import { chooseOption, expectToast, UI_TIMEOUT } from "./ui"

/**
 * 采购责任规则。spec 不要再复制 ensureDefaultProcurementOwner：
 *   import { ensureDefaultProcurementOwner } from "../helpers/procurement"
 *   await ensureDefaultProcurementOwner(page)
 *
 * 生产 id / 文案：
 * - `/master-data/procurement-responsibilities` heading「采购责任规则」
 * - `#procurement-responsibility-rules-create` / `#procurement-responsibility-rules-empty-create`「新增规则」
 * - 对话框「新增采购责任规则」：
 *   `#procurement-responsibility-rules-dialog-rule-type`（默认调度人 = DEFAULT_DISPATCHER）
 *   `#procurement-responsibility-rules-dialog-owner`（选项文案「采购 · caigou」）
 *   `#procurement-responsibility-rules-dialog-save`「保存规则」
 * - 成功 toast「采购责任规则已新增」或「采购责任规则已更新」
 * - 列表 `#procurement-responsibility-rules-table` 单元格「默认调度人」
 */

/**
 * 保证存在「默认调度人」规则，否则销售提交实物单无法解析采购负责人。
 * 已存在则直接返回。
 */
export async function ensureDefaultProcurementOwner(page: Page): Promise<void> {
    await page.goto("/master-data/procurement-responsibilities")
    await expect(page.getByRole("heading", { name: "采购责任规则" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    await page
        .waitForResponse(
            (response) =>
                response.request().method() === "GET" &&
                response.url().includes("procurement-responsibility-rules"),
            { timeout: UI_TIMEOUT },
        )
        .catch(() => undefined)

    await expect(
        page
            .locator("#procurement-responsibility-rules-table")
            .or(page.getByText("还没有采购责任规则"))
            .first(),
    ).toBeVisible({ timeout: UI_TIMEOUT })

    if (await page.getByText("默认调度人").count()) return

    const create = page.locator("#procurement-responsibility-rules-create")
    const emptyCreate = page.locator("#procurement-responsibility-rules-empty-create")
    if (await page.getByText("规则编辑依赖加载失败").count()) {
        throw new Error("采购责任规则依赖加载失败，无法新增默认调度人")
    }
    if (await create.count()) {
        try {
            await expect(create).toBeEnabled({ timeout: UI_TIMEOUT })
        } catch {
            throw new Error("无法新增采购责任规则：新增按钮仍不可用")
        }
        await create.click()
    } else if (await emptyCreate.count()) {
        await emptyCreate.click()
    } else {
        await page.getByRole("button", { name: "新增规则" }).click()
    }

    const dialog = page.getByRole("dialog", { name: "新增采购责任规则" })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-rule-type"),
        "默认调度人",
        "默认",
    )
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-owner"),
        /采购|caigou/,
        "caigou",
    )
    const save = dialog.locator("#procurement-responsibility-rules-dialog-save")
    if (await save.count()) {
        await expect(save).toBeEnabled({ timeout: UI_TIMEOUT })
        await save.click()
    } else {
        await dialog.getByRole("button", { name: "保存规则" }).click()
    }
    await expectToast(page, /采购责任规则已新增|采购责任规则已更新/)
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT })
}

/**
 * 实物销售单必须等采购负责人匹配完成再点「提交审批」。
 * 匹配进行中点击会被表单吞掉，对话框「提交销售单」不会出现。
 */
export async function submitCreatedSalesOrder(page: Page): Promise<void> {
    const owners = page.locator('[data-testid^="sales-line-procurement-owner-"]')
    await expect(owners.first()).toBeVisible({ timeout: UI_TIMEOUT })
    await expect
        .poll(
            async () => {
                const texts = (await owners.allTextContents().catch(() => [])).map(
                    (text) => text.replace(/\s+/g, " ").trim(),
                )
                if (
                    texts.some(
                        (text) =>
                            text.length > 0 &&
                            !/匹配失败|待配置|暂时无法匹配|正在匹配|选择商品|暂不能/.test(
                                text,
                            ),
                    )
                ) {
                    return "ok"
                }
                if (texts.some((text) => /匹配失败|待配置|暂时无法匹配/.test(text))) {
                    return `failed:${texts.join(" | ")}`
                }
                return "wait"
            },
            { timeout: UI_TIMEOUT, intervals: [200, 400, 800] },
        )
        .toBe("ok")

    await page.locator("#sales-orders-create-submit").click()
    await expect(page.getByRole("dialog", { name: "提交销售单" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}
