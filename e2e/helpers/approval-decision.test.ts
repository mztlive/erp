import { expect, test, type Page } from "@playwright/test"

import { approveCurrentDocument } from "./ui"

const HELPER_ORIGIN = "https://e2e-helper.invalid"

/** 模拟当前共用审批对话框和真实响应信封，不连接 ERP。 */
async function mockApprovalDecision(
    page: Page,
    reply: { status: number; body: object },
): Promise<() => number> {
    let decisions = 0
    await page.route(
        `${HELPER_ORIGIN}/admin/approval-decisions`,
        async (route) => {
            expect(route.request().method()).toBe("POST")
            decisions += 1
            await route.fulfill({
                status: reply.status,
                contentType: "application/json",
                body: JSON.stringify(reply.body),
            })
        },
    )
    await page.route(`${HELPER_ORIGIN}/workspace`, async (route) => {
        await route.fulfill({
            contentType: "text/html; charset=utf-8",
            body: `
                <button id="document-approve">通过</button>
                <div role="dialog" aria-label="确认通过" hidden>
                    <button id="document-decision-dialog-submit">确认通过</button>
                </div>
                <script>
                    const dialog = document.querySelector('[role="dialog"]');
                    document.getElementById('document-approve').addEventListener('click', () => dialog.hidden = false);
                    document.getElementById('document-decision-dialog-submit').addEventListener('click', async () => {
                        const response = await fetch('/admin/approval-decisions', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ decision: 'APPROVE', work_item_id: 'helper-task', expected_task_version: 1 }),
                        });
                        const body = await response.json();
                        if (response.ok && body.success) dialog.hidden = true;
                    });
                </script>
            `,
        })
    })
    await page.goto(`${HELPER_ORIGIN}/workspace`)
    return () => decisions
}

for (const reply of [
    {
        status: 409,
        body: {
            status: 409,
            success: false,
            errorMessage: "审批任务版本已变化，请刷新后重试",
        },
    },
    {
        status: 200,
        body: {
            status: 409,
            success: false,
            errorMessage: "审批任务版本已变化，请刷新后重试",
        },
    },
]) {
    test(`审批失败立即报告 HTTP ${reply.status} 与业务错误，不等待弹窗关闭或重试`, async ({
        page,
    }) => {
        const decisions = await mockApprovalDecision(page, reply)
        const started = Date.now()
        await expect(approveCurrentDocument(page)).rejects.toThrow(
            "审批任务版本已变化，请刷新后重试",
        )
        expect(decisions()).toBe(1)
        expect(Date.now() - started).toBeLessThan(3_000)
        await expect(
            page.getByRole("dialog", { name: "确认通过", exact: true }),
        ).toBeVisible()
    })
}

test("成功审批先确认决定响应，再等待对话框关闭", async ({ page }) => {
    const decisions = await mockApprovalDecision(page, {
        status: 200,
        body: {
            status: 200,
            success: true,
            data: { instance_status: "APPROVED" },
        },
    })
    await approveCurrentDocument(page)
    expect(decisions()).toBe(1)
    await expect(
        page.getByRole("dialog", { name: "确认通过", exact: true }),
    ).toBeHidden()
})
