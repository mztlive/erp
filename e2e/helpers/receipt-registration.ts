import { expect, test, type Page, type Request } from "./test"

/** 使用两笔各 644 元的真实应收分配验证 1500 元回款草稿，最终保留 212 元待核销。 */
export async function assertReceiptRegistrationDraft(
    page: Page,
    input: {
        customerName: string
        orderNos: readonly [string, string]
        bankReference: string
    },
): Promise<void> {
    const table = page.locator("#customer-receivables-session-allocations")
    const rowA = table.getByRole("row").filter({ hasText: input.orderNos[0] })
    const rowB = table.getByRole("row").filter({ hasText: input.orderNos[1] })
    const amountA = rowA.locator('input[id$="-amount"]')
    const amountB = rowB.locator('input[id$="-amount"]')
    const receiptAmount = page.locator("#customer-receivables-session-amount")
    const search = page.locator(
        "#customer-receivables-session-allocation-search",
    )
    const submit = page.locator("#customer-receivables-session-submit")
    const footer = page.locator('[data-slot="sticky-total-bar"]')
    const validation = page.locator("#customer-receivables-session-validation")
    const commits: Request[] = []
    const pageErrors: string[] = []
    const consoleErrors: string[] = []
    const onPageError = (error: Error) => pageErrors.push(error.message)
    const onConsole = (message: import("@playwright/test").ConsoleMessage) => {
        if (message.type() === "error") consoleErrors.push(message.text())
    }
    const onRequest = (request: Request) => {
        if (
            request.method() === "POST" &&
            request.url().includes("/admin/customer-receipts/commit")
        ) {
            commits.push(request)
        }
    }
    page.on("request", onRequest)
    page.on("pageerror", onPageError)
    page.on("console", onConsole)
    const initialViewport = page.viewportSize()

    try {
        await test.step("主体锁定、搜索保留选择与核销金额", async () => {
            const party = page.locator(
                "#customer-receivables-session-counterparty",
            )
            await expect(party).toHaveValue(input.customerName)
            await expect(party).toHaveJSProperty("readOnly", true)
            await search.fill(input.orderNos[1])
            await expect(rowA).toHaveCount(0)
            await expect(rowB.getByRole("checkbox")).toBeChecked()
            await expect(table).toContainText("已选 2 笔")
            await search.fill("不存在的应收项目")
            await expect(table).toContainText("已选分配保持不变")
            await expect(table).toContainText("已选 2 笔")
            await search.clear()
            await expect(amountA).toHaveValue("644.00")
            await expect(amountB).toHaveValue("644.00")
        })

        await test.step("非法金额、超额及搜索结果外的错误定位", async () => {
            await amountA.fill("-1.00")
            await expect(submit).toBeDisabled()
            await expect(amountA).toHaveAttribute("aria-invalid", "true")
            await search.fill(input.orderNos[1])
            await validation
                .getByRole("button")
                .filter({ hasText: "核销金额不能为负" })
                .click()
            await expect(search).toHaveValue("")
            await expect(amountA).toBeFocused()
            for (const invalid of ["abc", "12.345"]) {
                await amountA.fill(invalid)
                await expect(rowA).toContainText("请填写有效的核销金额")
                await expect(submit).toBeDisabled()
            }
            await amountA.fill("1288.01")
            await expect(rowA).toContainText("核销金额不可超过应收项目金额")
            await expect(submit).toBeDisabled()
            await amountA.fill("644.00")
            await amountA.fill("0.00")
            await amountB.fill("0.00")
            await expect(validation).toContainText(
                "提交审批至少需要一条核销分配",
            )
            await expect(submit).toBeDisabled()
            await amountA.fill("644.00")
            await amountB.fill("644.00")
            await receiptAmount.fill("1000.00")
            await expect(footer).toContainText("超出回款金额")
            await expect(
                footer.getByText("超出回款金额", { exact: true }).locator(".."),
            ).toContainText("288.00")
            await expect(submit).toBeDisabled()
            await receiptAmount.fill("12.345")
            await expect(validation).toContainText("请填写有效的非负金额")
            await expect(submit).toBeDisabled()
            await receiptAmount.fill("1500.00")
        })

        await test.step("取消关联可撤销，允许保留待核销余额", async () => {
            await rowB.getByRole("checkbox").uncheck()
            await expect(amountB).toHaveCount(0)
            await expect(
                footer.getByText("剩余待核销", { exact: true }).locator(".."),
            ).toContainText("856.00")
            await expect(submit).toBeEnabled()
            await page
                .locator("#customer-receivables-session-undo-remove")
                .click()
            await expect(rowB.getByRole("checkbox")).toBeChecked()
            await expect(amountB).toHaveValue("644.00")
            await expect(
                footer.getByText("剩余待核销", { exact: true }).locator(".."),
            ).toContainText("212.00")
            await expect(submit).toBeEnabled()
        })

        await test.step("保存草稿及未保存更改离开保护", async () => {
            await page
                .locator("#customer-receivables-session-save-draft")
                .click()
            await expect(page.getByText(/草稿已保存/)).toBeVisible()
            await expect(amountA).toHaveValue("644.00")
            await expect(amountB).toHaveValue("644.00")
            const bankReference = page.locator(
                "#customer-receivables-session-bank-reference",
            )
            await bankReference.fill(`${input.bankReference}-未保存`)
            await page.locator("#customer-receivables-session-close").click()
            await expect(page.getByRole("alertdialog")).toContainText(
                "回款登记尚有未保存的更改",
            )
            await page
                .locator("#customer-receivables-session-discard-dialog-cancel")
                .click()
            await expect(bankReference).toHaveValue(
                `${input.bankReference}-未保存`,
            )
            await bankReference.fill(input.bankReference)
        })

        await test.step("审批确认展示主体、金额与待核销余额，取消不提交", async () => {
            await submit.click()
            const confirm = page.getByRole("alertdialog")
            await expect(confirm).toContainText(input.customerName)
            await expect(confirm).toContainText("1,500.00")
            await expect(confirm).toContainText("1,288.00")
            await expect(confirm).toContainText("212.00")
            const path = test
                .info()
                .outputPath("receipt-registration-confirm.png")
            await page.screenshot({ path, animations: "disabled" })
            await test
                .info()
                .attach("回款提交确认", { path, contentType: "image/png" })
            await page
                .locator(
                    "#customer-receivables-session-receipt-confirm-dialog-cancel",
                )
                .click()
            await expect(confirm).toHaveCount(0)
            expect(
                commits,
                "保存草稿与取消确认均不得提交真实回款",
            ).toHaveLength(0)
        })

        await test.step("桌面与窄窗口回款表单、底部操作可达", async () => {
            for (const viewport of [
                { width: 1440, height: 1024 },
                { width: 1024, height: 768 },
                { width: 768, height: 1024 },
            ]) {
                await page.setViewportSize(viewport)
                await expect(submit).toBeInViewport()
                const dimensions = await page.evaluate(() => ({
                    width: document.documentElement.clientWidth,
                    scrollWidth: document.documentElement.scrollWidth,
                }))
                expect(
                    dimensions.scrollWidth,
                    "页面不得横向撑破；表格可在内部滚动",
                ).toBeLessThanOrEqual(dimensions.width)
                const boxes = await footer
                    .locator('[data-slot="money-value"]')
                    .all()
                const bounds = await Promise.all(
                    boxes.map((value) => value.boundingBox()),
                )
                for (let index = 1; index < bounds.length; index++) {
                    const previous = bounds[index - 1]
                    const current = bounds[index]
                    expect(previous).not.toBeNull()
                    expect(current).not.toBeNull()
                    if (
                        previous &&
                        current &&
                        Math.abs(previous.y - current.y) < 2
                    ) {
                        expect(
                            previous.x + previous.width + 8,
                            "底部金额之间须保留间距，不得重叠",
                        ).toBeLessThanOrEqual(current.x)
                    }
                }
                const path = test
                    .info()
                    .outputPath(
                        `receipt-registration-${viewport.width}x${viewport.height}.png`,
                    )
                await page.screenshot({ path, animations: "disabled" })
                await test
                    .info()
                    .attach(`回款登记 ${viewport.width}x${viewport.height}`, {
                        path,
                        contentType: "image/png",
                    })
            }
        })
        expect(pageErrors, "回款登记交互不得出现未处理的页面异常").toEqual([])
        expect(consoleErrors, "回款登记交互不得出现浏览器控制台错误").toEqual(
            [],
        )
    } finally {
        page.off("request", onRequest)
        page.off("pageerror", onPageError)
        page.off("console", onConsole)
        if (initialViewport) await page.setViewportSize(initialViewport)
    }
}
