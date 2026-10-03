import { readFileSync } from "node:fs"
import { expect, type Page, type Response } from "@playwright/test"

import { API_BASE, apiToken } from "./api"

const TIMEOUT = 20_000

export type InvoiceEvidenceFile = {
    name: string
    mimeType: string
    buffer: Buffer
}

type FinancialFile = {
    document_id: string
    document_no: string
    file_asset_id: string
    file_name: string
    content_type: string
    byte_size: number
}

/** 使用真实 PDF/PNG 内容，登记后逐字节核验下载结果。 */
export function invoiceEvidenceFiles(invoiceNo: string): InvoiceEvidenceFile[] {
    return [
        {
            name: `${invoiceNo}.pdf`,
            mimeType: "application/pdf",
            buffer: readFileSync(
                new URL("../fixtures/sample-contract.pdf", import.meta.url),
            ),
        },
        {
            name: `${invoiceNo}.png`,
            mimeType: "image/png",
            buffer: Buffer.from(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=",
                "base64",
            ),
        },
    ]
}

/** 选择附件后仍由正式登记命令一次提交文件和财务事实。 */
export async function uploadInvoiceEvidence(
    page: Page,
    files: InvoiceEvidenceFile[],
): Promise<void> {
    await page
        .locator("#customer-receivables-session-invoice-files-input")
        .setInputFiles(files)
    for (const file of files) {
        await expect(page.getByLabel(file.name, { exact: true })).toBeVisible({
            timeout: TIMEOUT,
        })
    }
}

/** 对照真实 multipart 请求证明附件引用与文件内容同时提交。 */
export async function expectInvoiceEvidenceCommit(
    response: Response,
    files: InvoiceEvidenceFile[],
): Promise<string> {
    expect(response.ok(), await response.text()).toBeTruthy()
    expect(new URL(response.url()).pathname).toBe(
        "/admin/invoices/commit-with-files",
    )
    const request = response.request()
    expect(request.headers()["content-type"]).toMatch(
        /^multipart\/form-data; boundary=/,
    )
    const payload = request.postDataBuffer()
    expect(payload).not.toBeNull()
    const serialized = payload!.toString("utf8")
    const commandPart = serialized.match(/name="command"\r\n\r\n([^\r]*)/)
    expect(commandPart, "multipart 应包含 command JSON").not.toBeNull()
    const command = JSON.parse(commandPart![1]) as {
        work_item_id?: string
        allocations?: unknown[]
        attachment_asset_ids?: string[]
    }
    expect(command.work_item_id).toBeTruthy()
    expect(command.allocations).toHaveLength(1)
    expect(command.attachment_asset_ids).toEqual(
        files.map(
            (file) =>
                `pending-file:invoice:${encodeURIComponent(file.name)}:${file.buffer.length}`,
        ),
    )
    for (const file of files) {
        expect(serialized).toContain(`filename="${file.name}"`)
        expect(
            payload!.includes(file.buffer),
            `${file.name} 应包含真实文件内容`,
        ).toBeTruthy()
    }
    const body = (await response.json()) as {
        success: boolean
        data: { id: string; invoice_no: string }
    }
    expect(body.success).toBe(true)
    expect(body.data.id).toBeTruthy()
    return body.data.id
}

/** 销售按本单详情资格下载财务附件；同客户其他销售单不获得附件资格。 */
export async function expectSalesInvoiceEvidence(
    page: Page,
    input: {
        salesOrderId: string
        invoiceId: string
        invoiceNo: string
        files: InvoiceEvidenceFile[]
        otherSalesOrderId?: string
    },
): Promise<void> {
    const token = await apiToken("xiaoshou")
    const headers = { Authorization: `Bearer ${token}` }
    const listPath = `/admin/sales-orders/${input.salesOrderId}/invoice-files`
    const listed = await page.request.get(`${API_BASE}${listPath}`, { headers })
    expect(listed.ok(), await listed.text()).toBeTruthy()
    const body = (await listed.json()) as {
        success: boolean
        data: FinancialFile[]
    }
    expect(body.success).toBe(true)
    expect(body.data).toHaveLength(input.files.length)
    await page.goto(`/sales/orders/${input.salesOrderId}?section=receivable`)
    await expect(
        page.getByRole("heading", { name: "发票文件", exact: true }),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    for (const expected of input.files) {
        const file = body.data.find((item) => item.file_name === expected.name)
        expect(file).toMatchObject({
            document_id: input.invoiceId,
            document_no: input.invoiceNo,
            content_type: expected.mimeType,
            byte_size: expected.buffer.length,
        })
        const downloadPath = `${listPath}/${input.invoiceId}/${file!.file_asset_id}/download`
        const row = page
            .getByRole("listitem")
            .filter({ hasText: expected.name })
        await expect(row).toContainText(input.invoiceNo, { timeout: TIMEOUT })
        const [download, response] = await Promise.all([
            page.waitForEvent("download"),
            page.waitForResponse(
                (result) => new URL(result.url()).pathname === downloadPath,
            ),
            row.getByRole("button", { name: "下载", exact: true }).click(),
        ])
        expect(download.suggestedFilename()).toBe(expected.name)
        expect(await download.failure()).toBeNull()
        expect(response.ok()).toBeTruthy()
        expect(response.headers()["content-type"]).toBe(expected.mimeType)
        expect(response.headers()["cache-control"]).toBe("private, no-store")
        const downloadedPath = await download.path()
        expect(downloadedPath, "浏览器下载应生成真实文件").not.toBeNull()
        expect(readFileSync(downloadedPath!)).toEqual(expected.buffer)
        if (input.otherSalesOrderId) {
            const foreign = await page.request.get(
                `${API_BASE}/admin/sales-orders/${input.otherSalesOrderId}/invoice-files/${input.invoiceId}/${file!.file_asset_id}/download`,
                { headers },
            )
            expect(foreign.status(), await foreign.text()).toBe(404)
        }
    }
    if (input.otherSalesOrderId) {
        const other = await page.request.get(
            `${API_BASE}/admin/sales-orders/${input.otherSalesOrderId}/invoice-files`,
            { headers },
        )
        expect(other.ok(), await other.text()).toBeTruthy()
        expect((await other.json()).data).toEqual([])
    }
}

/**
 * 销售在销售单票款页提交开票申请。批准后才生成 W01 销项开票任务。
 */
export async function submitSalesInvoiceRequest(
    page: Page,
    input: {
        salesOrderId: string
        amount: string
        taxNumber: string
        title?: string
        content?: string
        reason?: string
    },
): Promise<void> {
    await page.goto(`/sales/orders/${input.salesOrderId}?section=receivable`)
    const create = page.locator("#invoice-request-create")
    await expect(create).toBeEnabled({ timeout: TIMEOUT })
    await create.click()
    await expect(page.getByRole("heading", { name: "申请开票" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText(/本单可申请/)).toBeVisible({ timeout: TIMEOUT })

    await page.locator("#invoice-request-amount").fill(input.amount)
    if (input.title) {
        await page.locator("#invoice-request-title").fill(input.title)
    } else {
        await expect(page.locator("#invoice-request-title")).not.toHaveValue("")
    }
    await page.locator("#invoice-request-tax-number").fill(input.taxNumber)
    await page.locator("#invoice-request-content").fill(input.content ?? "商品")
    await page
        .locator("#invoice-request-reason")
        .fill(input.reason ?? "销售单已生效，申请开票")

    const submit = page.locator("#invoice-request-submit")
    await expect(submit).toBeEnabled({ timeout: TIMEOUT })
    const posted = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/sales-invoice-requests/submit"),
        { timeout: 60_000 },
    )
    await submit.click({ force: true })
    const response = await posted
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(
        page
            .locator('[aria-label="开票申请详情"]')
            .or(page.getByText("正在读取开票申请"))
            .or(page.getByRole("button", { name: "撤回申请" }))
            .first(),
    ).toBeVisible({ timeout: TIMEOUT })
    await expect(page.locator("#invoice-request-form")).toHaveCount(0)
    await expect(page.locator("#invoice-request-submit")).toHaveCount(0)
}
