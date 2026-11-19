import { workPage } from "./workspace-page"
import { expect, type Page } from "@playwright/test"

import { API_BASE, apiGet, apiToken } from "./api"
import { UI_TIMEOUT } from "./ui"

type DispatchRule = { rule_type: string; status: string; owner_user_ids: string[] }

/** Verify the seeded default dispatcher. The responsibility-rule UI has its own E2E flow. */
export async function ensureDefaultProcurementOwner(_page: Page): Promise<void> {
    const adminToken = await apiToken("admin")
    const { userid } = await apiGet<{ userid: string }>(await apiToken("caigou"), "/account/profile")
    const path = "/admin/procurement-responsibility-rules"
    const { items } = await apiGet<{ items: DispatchRule[] }>(adminToken, path, { page: 1, page_size: 200 })
    const rule = items.find((row) => row.rule_type === "DEFAULT_DISPATCHER" && row.status === "active")
    if (rule) {
        expect(rule.owner_user_ids, "E2E 默认分派组应包含采购账号 caigou").toContain(userid)
        return
    }
    if (process.env.ERP_E2E_ISOLATED !== "1") throw new Error("请先初始化隔离测试库的默认采购分派组")
    const response = await fetch(`${API_BASE}${path}`, {
        method: "POST",
        headers: { Authorization: `Bearer ${adminToken}`, "Content-Type": "application/json" },
        body: JSON.stringify({
            rule_type: "DEFAULT_DISPATCHER",
            owner_user_ids: [userid],
            strategy: "ROUND_ROBIN",
            status: "active",
        }),
        signal: AbortSignal.timeout(UI_TIMEOUT),
    })
    const result = await response.json()
    expect(response.ok && result.success, result.errorMessage).toBe(true)
}

/**
 * 实物销售单必须等采购负责人匹配完成再点「提交审批」。
 * 匹配进行中点击会被表单吞掉，对话框「提交销售单」不会出现。
 */
export async function submitCreatedSalesOrder(page: Page): Promise<void> {
    const owners = workPage(page).locator('[data-testid^="sales-line-procurement-owner-"]')
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

    await workPage(page).locator("#sales-orders-create-submit").click()
    await expect(workPage(page).getByRole("dialog", { name: "提交销售单" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}
