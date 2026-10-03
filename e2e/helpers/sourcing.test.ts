import { expect, test, type Page } from "@playwright/test"

import { confirmSupplyAllocation } from "./sourcing"

const HELPER_ORIGIN = "https://e2e-helper.invalid"

/** 模拟当前预览、失败提示与提交接口，不连接 ERP。 */
async function mockSupplyAllocation(
    page: Page,
    options: {
        previewOpen: boolean
        reply?: { status: number; body: object }
    },
): Promise<() => number> {
    let commits = 0
    await page.route(
        `${HELPER_ORIGIN}/admin/purchase-orders/from-sourcing`,
        async (route) => {
            expect(route.request().method()).toBe("POST")
            commits += 1
            await route.fulfill({
                status: options.reply?.status ?? 200,
                contentType: "application/json",
                body: JSON.stringify(
                    options.reply?.body ?? {
                        status: 200,
                        success: true,
                        data: {
                            orders: [],
                            stock_reservations: [
                                { sales_order_line_id: "line-A" },
                            ],
                            work_item_status: "COMPLETED",
                        },
                    },
                ),
            })
        },
    )
    await page.route(`${HELPER_ORIGIN}/workspace`, async (route) => {
        await route.fulfill({
            contentType: "text/html; charset=utf-8",
            body: `
                <button id="procurement-orders-create-preview">预览供给分配</button>
                <div role="dialog" aria-label="无法预览供给分配" data-slot="toast">
                    拆分数量合计不能超过销售数量
                </div>
                <div id="preview-overlay" ${options.previewOpen ? "" : "hidden"}
                    style="position:fixed;inset:0;z-index:10;background:#ddd"></div>
                <div role="dialog" aria-label="预览供给分配" data-slot="dialog-content"
                    ${options.previewOpen ? "" : "hidden"}
                    style="position:fixed;top:50%;left:50%;z-index:20">
                    <button id="procurement-orders-create-preview-confirm">确认库存分配</button>
                </div>
                <script>
                    window.previewClicks = 0;
                    const preview = document.querySelector('[data-slot="dialog-content"]');
                    const overlay = document.getElementById('preview-overlay');
                    document.getElementById('procurement-orders-create-preview').addEventListener('click', async () => {
                        window.previewClicks += 1;
                        await Promise.resolve();
                        preview.hidden = false;
                        overlay.hidden = false;
                    });
                    document.getElementById('procurement-orders-create-preview-confirm').addEventListener('click', async () => {
                        const response = await fetch('/admin/purchase-orders/from-sourcing', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ sales_order_id: 'sales-A', lines: [{ sales_order_line_id: 'line-A' }] }),
                        });
                        const body = await response.json();
                        if (!response.ok || !body.success) return;
                        preview.hidden = true;
                        overlay.hidden = true;
                        const toast = document.createElement('div');
                        toast.setAttribute('data-slot', 'toast');
                        toast.textContent = '供给分配已完成';
                        document.body.appendChild(toast);
                    });
                </script>
            `,
        })
    })
    await page.goto(`${HELPER_ORIGIN}/workspace`)
    return () => commits
}

for (const previewOpen of [true, false]) {
    test(
        previewOpen
            ? "错误提示与正式预览同时存在时，直接确认正式预览且不重复点击底层按钮"
            : "预览尚未打开时，打开一次并等待正式预览后提交",
        async ({ page }) => {
            const commits = await mockSupplyAllocation(page, { previewOpen })
            await confirmSupplyAllocation(page)
            expect(commits()).toBe(1)
            expect(
                await page.evaluate(
                    () =>
                        (window as unknown as { previewClicks: number })
                            .previewClicks,
                ),
            ).toBe(previewOpen ? 0 : 1)
            await expect(
                page.getByRole("dialog", {
                    name: "预览供给分配",
                    exact: true,
                }),
            ).toBeHidden()
            await expect(
                page.getByRole("dialog", {
                    name: "无法预览供给分配",
                    exact: true,
                }),
            ).toBeVisible()
        },
    )
}

test("供给分配业务拒绝立即报告，不等待成功提示或重新提交", async ({ page }) => {
    const commits = await mockSupplyAllocation(page, {
        previewOpen: true,
        reply: {
            status: 200,
            body: {
                status: 409,
                success: false,
                errorMessage: "供给分配任务版本已变化",
            },
        },
    })
    await expect(confirmSupplyAllocation(page)).rejects.toThrow(
        "供给分配任务版本已变化",
    )
    expect(commits()).toBe(1)
    await expect(
        page.getByRole("dialog", { name: "预览供给分配", exact: true }),
    ).toBeVisible()
})
