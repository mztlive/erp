import { readFileSync } from "node:fs"
import { expect, type Page, type Response } from "@playwright/test"

import { API_BASE, apiGet, apiToken } from "./api"
import { openWorkspaceTask } from "./ui"

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
export async function expectInvoiceEvidenceComplete(
    response: Response,
    files: InvoiceEvidenceFile[],
    amount: string,
): Promise<string> {
    expect(response.ok(), await response.text()).toBeTruthy()
    expect(new URL(response.url()).pathname).toBe(
        "/admin/invoices/complete-with-files",
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
        expected_task_version?: string
        expected_request_version?: number
        attachment_asset_ids?: string[]
    }
    expect(command.work_item_id).toBeTruthy()
    expect(command.expected_task_version).toBeTruthy()
    expect(command.expected_request_version).toBeGreaterThan(0)
    expect(Object.keys(command).sort()).toEqual([
        "attachment_asset_ids", "expected_request_version", "expected_task_version",
        "idempotency_key", "work_item_id",
    ])
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
        data: { id: string; unallocated_amount: string; allocations: unknown[] }
    }
    expect(body.success).toBe(true)
    expect(body.data.id).toBeTruthy()
    expect(body.data).toMatchObject({
        invoice_no: null, invoice_date: null, net_amount: null, tax_amount: null,
        gross_amount: amount, allocated_total: amount,
    })
    expect(body.data.unallocated_amount).toMatch(/^0(?:\.0+)?$/)
    expect(body.data.allocations).toHaveLength(1)
    expect(body.data.allocations[0]).toMatchObject({
        allocated_gross_amount: amount, allocated_net_amount: null, allocated_tax_amount: null,
    })
    return body.data.id
}

/** W01 只上传附件，由服务端按剩余批准额度一次完成开票。 */
export async function completeSalesInvoiceFromWorkspace(
    page: Page,
    input: { orderNo: string; customerName: string; amount: string; evidenceName: string },
) {
    await openWorkspaceTask(page, "销项开票", input.customerName, "finance")
    const task = page.getByLabel("当前开票任务", { exact: true })
    await expect(task).toBeVisible({ timeout: TIMEOUT })
    await expect(page.locator("#workspace-invoice-source-sales")).toContainText(input.orderNo)
    await expect(task.getByRole("heading", { name: "实际发票信息" })).toBeVisible()
    await expect(page.locator("#customer-receivables-session-invoice-no")).toHaveCount(0)
    await expect(page.locator("#customer-receivables-session-gross-amount")).toHaveCount(0)
    const confirm = page.locator("#workspace-invoice-confirm")
    await expect(confirm).toBeDisabled()
    const files = invoiceEvidenceFiles(input.evidenceName)
    await uploadInvoiceEvidence(page, files)
    await expect(confirm).toBeEnabled()
    const completed = page.waitForResponse(
        response => response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/invoices/complete-with-files",
        { timeout: 90_000 },
    )
    await confirm.click()
    const invoiceId = await expectInvoiceEvidenceComplete(await completed, files, input.amount)
    await expect(task).toBeHidden({ timeout: TIMEOUT })
    return { invoiceId, invoiceNo: null, files }
}

/** 红冲必须有完整票面事实；使用当前开票员及正式任务登记蓝票，不直接写库。 */
export async function registerDetailedSalesInvoice(
    page: Page,
    input: { orderNo: string; customerName: string; invoiceNo: string; amount: string; netAmount: string; taxAmount: string },
) {
    await openWorkspaceTask(page, "销项开票", input.customerName, "finance")
    await expect(page.getByLabel("当前开票任务", { exact: true })).toBeVisible({ timeout: TIMEOUT })
    await expect(page.locator("#workspace-invoice-source-sales")).toContainText(input.orderNo)
    const taskId = new URL(page.url()).searchParams.get("currentWorkItemId")
    expect(taskId).toBeTruthy()
    const token = await apiToken("kaipiao")
    const task = await apiGet<{
        id: string; task_version: string; business_object_type: string;
        business_object_id: string; work_item_type: string; status: string;
    }>(token, `/admin/work-items/${taskId}`)
    expect(task).toMatchObject({
        id: taskId, business_object_type: "receivable_account",
        work_item_type: "SALES_INVOICE_EXECUTION", status: "OPEN",
    })
    const account = await apiGet<{ id: string; counterparty_party_id: string }>(
        token, `/admin/receivable-accounts/${task.business_object_id}`,
    )
    expect(account.id).toBe(task.business_object_id)
    expect(account.counterparty_party_id).toBeTruthy()
    const files = invoiceEvidenceFiles(input.invoiceNo).slice(0, 1)
    const references = files.map(file =>
        `pending-file:invoice:${encodeURIComponent(file.name)}:${file.buffer.length}`,
    )
    const command = {
        work_item_id: task.id,
        expected_task_version: String(task.task_version),
        invoice_id: null,
        expected_version: null,
        invoice: {
            invoice_direction: "sales", invoice_kind: "blue",
            party_id: account.counterparty_party_id,
            invoice_no: input.invoiceNo,
            invoice_date: new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Shanghai" }),
            gross_amount: input.amount, net_amount: input.netAmount, tax_amount: input.taxAmount,
        },
        allocations: [{
            receivable_account_id: task.business_object_id,
            allocated_gross_amount: input.amount,
            allocated_net_amount: input.netAmount,
            allocated_tax_amount: input.taxAmount,
        }],
        attachment_asset_ids: references,
        idempotency_key: `e2e-detailed-${input.invoiceNo}`,
    }
    const response = await page.request.post(`${API_BASE}/admin/invoices/commit-with-files`, {
        headers: { Authorization: `Bearer ${token}` },
        multipart: {
            command: JSON.stringify(command),
            ...Object.fromEntries(files.map((file, index) => [references[index], file])),
        },
        timeout: 90_000,
    })
    expect(response.ok(), await response.text()).toBeTruthy()
    const body = await response.json()
    expect(body.success).toBe(true)
    expect(body.data).toMatchObject({
        invoice_no: input.invoiceNo, gross_amount: input.amount,
        net_amount: input.netAmount, tax_amount: input.taxAmount, allocated_total: input.amount,
    })
    expect(body.data.id).toBeTruthy()
    return { invoiceId: body.data.id as string, invoiceNo: input.invoiceNo, files }
}

/** 销售按本单详情资格下载财务附件；同客户其他销售单不获得附件资格。 */
export async function expectSalesInvoiceEvidence(
    page: Page,
    input: {
        salesOrderId: string
        invoiceId: string
        invoiceNo: string | null
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
    await page.goto(`/sales/orders/${input.salesOrderId}?section=invoice`)
    await expect(
        page.getByRole("heading", { name: "发票文件", exact: true }),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    for (const expected of input.files) {
        const file = body.data.find((item) => item.file_name === expected.name)
        expect(file).toMatchObject({
            document_id: input.invoiceId,
            document_no: input.invoiceNo ?? "附件登记",
            content_type: expected.mimeType,
            byte_size: expected.buffer.length,
        })
        const downloadPath = `${listPath}/${input.invoiceId}/${file!.file_asset_id}/download`
        const row = page
            .getByRole("listitem")
            .filter({ hasText: expected.name })
        await expect(row).toContainText(input.invoiceNo ?? "附件登记", { timeout: TIMEOUT })
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
 * 销售在销售单发票页提交开票申请。批准后才生成 W01 销项开票任务。
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
    await page.goto(`/sales/orders/${input.salesOrderId}?section=invoice`)
    const create = page.locator("#invoice-request-create")
    await expect(create).toBeEnabled({ timeout: TIMEOUT })
    await create.click()
    await expect(page.getByRole("heading", { name: "申请开票" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText(/本单可申请/)).toBeVisible({ timeout: TIMEOUT })

    await page.locator("#invoice-request-amount").fill(input.amount)
    const title = page.locator("#invoice-request-title")
    const taxNumber = page.locator("#invoice-request-tax-number")
    await expect(title).toHaveAttribute("readonly", "")
    await expect(taxNumber).toHaveAttribute("readonly", "")
    if (input.title) {
        await expect(title).toHaveValue(input.title, { timeout: TIMEOUT })
    } else {
        await expect(title).not.toHaveValue("", { timeout: TIMEOUT })
    }
    await expect(taxNumber).toHaveValue(input.taxNumber, { timeout: TIMEOUT })
    const frozenTitle = await title.inputValue()
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
    expect(response.request().postDataJSON()).toMatchObject({
        data: {
            amount: input.amount,
            invoice_title: frozenTitle,
            tax_number: input.taxNumber,
        },
    })
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
