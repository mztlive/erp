import fs from "node:fs/promises"

import { expect, type Locator, type Page, type Response } from "./test"
import { API_BASE, apiGet, apiToken } from "./api"
import { ACCOUNTS } from "./accounts"

const VISIBLE = { timeout: 20_000 }

export type FrozenMaterial = {
    file_asset_id: string
    file_name: string
    content_type: string
    byte_size: number
}

type FrozenSource = {
    customer: string | null
    amount_label: string | null
    lines: Array<{ title: string; quantity: string | null }>
    more_count: number
    extra_sections: Array<{ label: string; value: string }>
}

type FrozenSalesLine = {
    id: string
    line_no: number
    item_name_snapshot: string
    quantity: string | null
    card_count: number | null
    unit_price_gross: string | null
    gross_amount: string
    tax_amount: string
}

type FrozenSalesBody = {
    customer_name: string
    contract_no: string | null
    settlement_party_name: string | null
    payment_term_name: string
    invoice_type: string
    gross_amount: string
    net_amount: string
    tax_amount: string
}

export type FrozenApprovalMaterials = {
    document_no: string
    document_type: string
    document_id: string
    subject_version: number
    sales_order?:
        | (FrozenSalesBody & {
              submission_no: number
              status: string
              lines: FrozenSalesLine[]
          })
        | null
    source_sales_orders: Array<{
        document_no: string
        revision: FrozenSalesBody & {
            id: string
            revision_no: number
            commercial_lines: FrozenSalesLine[]
        }
    }>
    display: {
        source: FrozenSource
        source_sales?: Array<{
            document_id: string
            document_no: string
            revision_id: string
            revision_no: number
            source: FrozenSource
        }>
    }
    attachments: FrozenMaterial[]
}

export type PurchaseSalesContext = {
    id: string
    sales_order_id: string
    lines: Array<{
        line_type: string
        sales_order_revision_line_id?: string | null
        unit_cost_gross?: string | null
    }>
    source_sales_order: {
        sales_order_id: string
        sales_order_no: string
        revision_id: string
        revision_no: number
        customer_name: string
        contract_no: string | null
        totals: { gross: string; net: string; tax: string }
        materials_unavailable: boolean
        lines: Array<{
            sales_order_revision_line_id: string
            item_name: string
            quantity: string
            unit_price_gross: string
            gross_amount: string
        }>
        materials: Array<FrozenMaterial & { kind: string }>
    } | null
}

async function apiWrite<T>(
    method: string,
    path: string,
    token: string,
    body?: unknown,
): Promise<T> {
    const response = await fetch(`${API_BASE}${path}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(15_000),
    })
    const result = (await response.json()) as {
        success: boolean
        errorMessage?: string
        data: T
    }
    if (!response.ok || result.success === false) {
        throw new Error(
            `${method} ${path} 失败（HTTP ${response.status}）: ${result.errorMessage ?? ""}`,
        )
    }
    return result.data
}

/** 仅隔离后端临时收窄通用读取，保留原采购范围和实际任务资格；调用方必须 finally 恢复。 */
export async function restrictSalesMaterialReader(
    account: "caigou" | "caiwu" | "caigoushenpi",
    runId: string,
): Promise<() => Promise<void>> {
    expect(
        process.env.ERP_E2E_ISOLATED === "1",
        "临时权限配置必须使用隔离 E2E 数据库",
    ).toBe(true)
    const token = await apiToken("admin")
    const accounts = await apiGet<
        Array<{ id: string; account: string; role_ids: string[] }>
    >(token, "/admin/admins")
    const login = ACCOUNTS[account].account
    const user = accounts.find((row) => row.account === login)
    expect(user, "隔离种子账号必须存在").toBeTruthy()
    const roles = await apiGet<Array<{ id: string; permissions: string[] }>>(
        token,
        "/admin/roles",
    )
    const original = [
        ...new Set(
            roles
                .filter((row) => user!.role_ids.includes(row.id))
                .flatMap((row) => row.permissions),
        ),
    ]
    const permissions = original.filter(
        (permission) =>
            !/^(sales_order|contract|file_asset|document_attachment):/.test(
                permission,
            ),
    )
    expect(permissions).toContain("approval_instance:read")
    expect(permissions).toContain("approval_instance:decide")
    expect(permissions).not.toContain("*:*")
    if (account === "caiwu" || account === "caigoushenpi")
        expect(permissions).toContain("purchase_order:detail")
    const roleId = await apiWrite<string>("POST", "/admin/roles", token, {
        name: `E2E精确销售材料-${account}-${runId}`,
        permissions,
    })
    try {
        await apiWrite("PUT", `/admin/admins/${user!.id}/role`, token, {
            role_ids: [roleId],
        })
    } catch (error) {
        await apiWrite("DELETE", `/admin/roles/${roleId}`, token)
        throw error
    }
    return async () => {
        await apiWrite("PUT", `/admin/admins/${user!.id}/role`, token, {
            role_ids: user!.role_ids,
        })
        await apiWrite("DELETE", `/admin/roles/${roleId}`, token)
    }
}

/** 通用读取必须被真实 RBAC 拒绝，不能把专用上下文授权扩成普通销售、合同或文件资格。 */
export async function expectGenericSalesMaterialReadsDenied(
    token: string,
    ids: {
        salesOrderId: string
        contractId?: string | null
        fileAssetId: string
    },
): Promise<void> {
    const paths = [
        `/admin/sales-orders/${ids.salesOrderId}`,
        `/admin/file-assets/${ids.fileAssetId}`,
        `/admin/file-assets/${ids.fileAssetId}/preview`,
        ...(ids.contractId ? [`/admin/contracts/${ids.contractId}`] : []),
    ]
    for (const path of paths) {
        const response = await fetch(`${API_BASE}${path}`, {
            headers: { Authorization: `Bearer ${token}` },
            signal: AbortSignal.timeout(15_000),
        })
        expect(response.status, `${path} 不得借审批/采购资格开放`).toBe(403)
        expect(((await response.json()) as { success: boolean }).success).toBe(
            false,
        )
    }
}

/** 从当前任务打开完整冻结销售纸张，核对提交或正式版本及全部明细。 */
export async function openFrozenApprovalMaterials(
    page: Page,
    sourceRevisionId?: string,
): Promise<{
    instanceId: string
    materials: FrozenApprovalMaterials
}> {
    const currentTask = page
        .getByRole("region", { name: "当前工作台任务", exact: true })
        .filter({ visible: true })
    await expect(currentTask).toHaveCount(1, VISIBLE)
    await expect(currentTask).toBeVisible(VISIBLE)
    const workItemId = new URL(page.url()).searchParams.get("currentWorkItemId")
    expect(workItemId, "冻结资料必须从已选定的当前审批任务打开").toBeTruthy()
    const action = sourceRevisionId
        ? currentTask
              .getByRole("button", { name: "预览销售单", exact: true })
              .and(
                  currentTask.locator(
                      `[id$="-source-${idSegment(sourceRevisionId)}"]`,
                  ),
              )
        : currentTask.getByRole("button", {
              name: "预览完整销售单",
              exact: true,
          })
    await expect(action.filter({ visible: true })).toHaveCount(1, VISIBLE)
    await expect(action).toBeVisible(VISIBLE)
    const actionId = await action.getAttribute("id")
    if (sourceRevisionId) {
        expect(actionId).toMatch(/^workspace-purchase-approval-.+-source-/)
    } else {
        expect(
            [
                `workspace-task-detail-read-${idSegment(workItemId!)}`,
                `workspace-task-detail-preview-submission-${idSegment(workItemId!)}`,
            ],
            "完整销售资料入口必须属于 URL 选定的当前审批任务",
        ).toContain(actionId)
    }
    const [response] = await Promise.all([
        page.waitForResponse(
            (item) =>
                item.request().method() === "GET" &&
                /\/admin\/approval-instances\/[^/]+\/materials$/.test(
                    new URL(item.url()).pathname,
                ),
        ),
        action.click(),
    ])
    expect(response.ok(), await response.text()).toBe(true)
    const instanceId = new URL(response.url()).pathname.split("/").at(-2)!
    if (sourceRevisionId) {
        expect(actionId).toBe(
            `workspace-purchase-approval-${idSegment(instanceId)}-source-${idSegment(sourceRevisionId)}`,
        )
    }
    const result = (await response.json()) as {
        success: boolean
        data: FrozenApprovalMaterials
    }
    expect(result.success).toBe(true)
    const dialog = page.getByRole("dialog", {
        name: sourceRevisionId ? "销售单预览" : /提交资料$/,
        exact: true,
    })
    await expect(dialog).toHaveCount(1, VISIBLE)
    await expect(dialog).toBeVisible(VISIBLE)
    const paper = dialog
        .getByRole("article", { name: "纸质单据预览", exact: true })
        .and(dialog.locator('[data-slot="paper-document"]'))
    await expect(paper).toHaveCount(1, VISIBLE)
    await expect(paper).toBeVisible(VISIBLE)
    const paperHeading = paper.getByRole("heading", {
        name: "销售单",
        level: 1,
        exact: true,
    })
    await expect(paperHeading).toHaveCount(1, VISIBLE)
    await expect(paperHeading).toBeVisible(VISIBLE)
    if (sourceRevisionId) {
        const allowed = result.data.display.source_sales ?? []
        expect(result.data.source_sales_orders).toHaveLength(allowed.length)
        for (const source of allowed) {
            const full = result.data.source_sales_orders.find(
                (order) => order.revision.id === source.revision_id,
            )
            expect(full, "冻结允许的每个销售版本必须返回完整正文").toBeTruthy()
            expect(full!.document_no).toBe(source.document_no)
            expect(full!.revision.revision_no).toBe(source.revision_no)
            expect(full!.revision.commercial_lines).toHaveLength(
                source.source.lines.length + source.source.more_count,
            )
        }
        const full = result.data.source_sales_orders.find(
            (order) => order.revision.id === sourceRevisionId,
        )
        expect(full, "预览必须使用指定的冻结正式销售版本").toBeTruthy()
        await expect(paper).toContainText(full!.document_no)
        await expect(paper).toContainText(
            `销售版本 V${full!.revision.revision_no}`,
        )
        await expectCompleteFrozenSalesPaper(
            paper,
            full!.revision,
            full!.revision.commercial_lines,
        )
    } else {
        const full = result.data.sales_order
        expect(full, "销售审批必须返回此次完整提交").toBeTruthy()
        expect(full!.submission_no).toBe(result.data.subject_version)
        expect(full!.lines).toHaveLength(
            result.data.display.source.lines.length +
                result.data.display.source.more_count,
        )
        await expect(paper).toContainText(result.data.document_no)
        await expect(paper).toContainText(`提交版本 ${full!.submission_no}`)
        await expectCompleteFrozenSalesPaper(paper, full!, full!.lines)
    }
    return { instanceId, materials: result.data }
}

async function expectCompleteFrozenSalesPaper(
    paper: Locator,
    body: FrozenSalesBody,
    lines: FrozenSalesLine[],
): Promise<void> {
    expect(lines.length, "完整销售纸张必须包含销售明细").toBeGreaterThan(0)
    for (const name of [
        body.customer_name,
        body.settlement_party_name,
        body.payment_term_name,
        body.invoice_type,
    ]) {
        expect(name, "销售正文的客户、我方主体和商务快照必须完整").toBeTruthy()
        await expect(paper).toContainText(name!)
    }
    if (body.contract_no) await expect(paper).toContainText(body.contract_no)
    const section = paper.getByRole("region", {
        name: `销售明细 · 共 ${lines.length} 行`,
        exact: true,
    })
    const rows = section.locator("tbody tr")
    await expect(rows).toHaveCount(lines.length)
    for (const [index, line] of lines.entries()) {
        const row = rows.nth(index)
        await expect(row.getByRole("cell").first()).toHaveText(
            String(line.line_no),
        )
        await expect(row).toContainText(line.item_name_snapshot)
        const quantity = line.quantity ?? String(line.card_count)
        await expect(
            row.locator('[data-slot="quantity-value"] > span').first(),
        ).toHaveText(decimalPattern(quantity))
        if (line.unit_price_gross) {
            await expect(
                row.locator('[data-slot="money-value-number"]').first(),
            ).toHaveText(displayedCurrency(line.unit_price_gross))
        }
        await expect(
            row.locator('[data-slot="money-value-number"]').last(),
        ).toHaveText(displayedCurrency(line.gross_amount))
        await expect(
            row.locator('[data-slot="money-value-number"]').nth(-2),
        ).toHaveText(displayedCurrency(line.tax_amount))
    }
    const totals = paper.getByRole("region", { name: "金额汇总", exact: true })
    for (const amount of [
        body.net_amount,
        body.tax_amount,
        body.gross_amount,
    ]) {
        await expect(totals).toContainText(displayedCurrency(amount))
    }
}

async function expectMaterialHeaders(
    response: Response,
    bytes: Buffer,
): Promise<void> {
    const error = response.ok() ? "" : await response.text()
    expect(response.status(), error).toBe(200)
    expect(response.headers()["cache-control"]).toContain("no-store")
    expect(response.headers()["x-content-type-options"]).toBe("nosniff")
    expect(response.headers()["content-length"]).toBe(String(bytes.length))
}

/** 从用户实际看到的 Blob 校验字节，避免 CDP 空响应回退发起缺少 JWT 的第二次读取。 */
async function expectPresentedBlob(
    preview: Locator,
    bytes: Buffer,
    image: boolean,
): Promise<void> {
    await expect(preview).toBeVisible(VISIBLE)
    if (image) {
        await expect(preview).toHaveJSProperty("complete", true, VISIBLE)
        await expect
            .poll(
                () =>
                    preview.evaluate(
                        (element) => (element as HTMLImageElement).naturalWidth,
                    ),
                VISIBLE,
            )
            .toBeGreaterThan(0)
    }
    const actual = await preview.evaluate(async (element) => {
        const source = element.getAttribute("src")
        if (!source?.startsWith("blob:"))
            throw new Error("预览未使用已授权文件 Blob")
        const response = await fetch(source)
        return Array.from(new Uint8Array(await response.arrayBuffer()))
    })
    expect(
        Buffer.from(actual),
        "实际呈现的文件 Blob 必须等于上传原文件",
    ).toEqual(bytes)
}

function idSegment(value: string): string {
    return value
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "")
}

function decimalPattern(value: string): RegExp {
    const escaped = value
        .replace(/\.0+$/, "")
        .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    return new RegExp(`^${escaped}(?:\\.0+)?$`)
}

/** 金额列按两位小数展示；使用整数计算从 API 成本推导对应显示值。 */
function displayedCurrency(value: string): string {
    const matched = /^(\d+)(?:\.(\d+))?$/.exec(value)
    if (!matched) throw new Error(`采购含税成本不是非负十进制字符串: ${value}`)
    const fraction = matched[2] ?? ""
    const cents =
        BigInt(matched[1]!) * 100n +
        BigInt(fraction.padEnd(2, "0").slice(0, 2)) +
        (fraction.charAt(2) >= "5" ? 1n : 0n)
    const integer = new Intl.NumberFormat("zh-CN", {
        style: "currency",
        currency: "CNY",
        minimumFractionDigits: 0,
        maximumFractionDigits: 0,
    }).format(cents / 100n)
    return `${integer}.${(cents % 100n).toString().padStart(2, "0")}`
}

/** 预览和下载均走当前实例白名单，校验真实响应和浏览器下载的完整文件内容。 */
export async function expectApprovalMaterialPreviewAndDownload(
    page: Page,
    options: {
        instanceId: string
        file: FrozenMaterial
        bytes: Buffer
    },
): Promise<void> {
    const { instanceId, file, bytes } = options
    expect(file.byte_size).toBe(bytes.length)
    const path = `/admin/approval-instances/${instanceId}/materials/${file.file_asset_id}`
    const prefix = `approval-submitted-material-${idSegment(file.file_asset_id)}`
    const [materialPage, response] = await Promise.all([
        page.context().waitForEvent("page"),
        page
            .context()
            .waitForEvent(
                "response",
                (item) =>
                    item.request().method() === "GET" &&
                    new URL(item.url()).pathname === `${path}/preview`,
            ),
        page.locator(`#${prefix}-preview`).click(),
    ])
    try {
        const previewUrl = new URL(materialPage.url())
        expect(previewUrl.pathname).toBe("/approval-materials")
        expect(previewUrl.searchParams.get("instance")).toBe(instanceId)
        expect(previewUrl.searchParams.get("file")).toBe(file.file_asset_id)
        await expectMaterialHeaders(response, bytes)
        expect(response.headers()["content-type"]).toBe(file.content_type)
        await expect(
            materialPage.getByRole("heading", {
                name: file.file_name,
                exact: true,
            }),
        ).toBeVisible(VISIBLE)
        await expectPresentedBlob(
            file.content_type === "application/pdf"
                ? materialPage.locator("#approval-material-preview-document")
                : materialPage.getByRole("img", {
                      name: file.file_name,
                      exact: true,
                  }),
            bytes,
            file.content_type !== "application/pdf",
        )
    } finally {
        await materialPage.close()
    }
    const [download, downloadedResponse] = await Promise.all([
        page.waitForEvent("download"),
        page.waitForResponse(
            (item) =>
                item.request().method() === "GET" &&
                new URL(item.url()).pathname === `${path}/download`,
        ),
        page.locator(`#${prefix}-download`).click(),
    ])
    await expectMaterialHeaders(downloadedResponse, bytes)
    expect(downloadedResponse.headers()["content-type"]).toBe(
        "application/octet-stream",
    )
    expect(await download.failure()).toBeNull()
    expect(download.suggestedFilename()).toBe(file.file_name)
    const downloadedPath = await download.path()
    expect(downloadedPath).toBeTruthy()
    expect(await fs.readFile(downloadedPath!)).toEqual(bytes)
}

/** 构造已上传、未关联该采购的真实 PDF 资产，验证上下文接口不能任意下载文件。 */
export async function uploadUnrelatedSalesMaterial(
    bytes: Buffer,
    runId: string,
): Promise<string> {
    const body = new FormData()
    const uniqueBytes = Buffer.concat([
        bytes,
        Buffer.from(`\n%E2E-unrelated-${runId}\n`),
    ])
    body.append(
        "file",
        new Blob([new Uint8Array(uniqueBytes)], { type: "application/pdf" }),
        `unrelated-${runId}.pdf`,
    )
    body.append("sensitivity_class", "sensitive")
    body.append("retention_class", "long_term")
    const response = await fetch(`${API_BASE}/admin/file-assets/upload`, {
        method: "POST",
        headers: { Authorization: `Bearer ${await apiToken("admin")}` },
        body,
        signal: AbortSignal.timeout(30_000),
    })
    expect(response.ok, await response.clone().text()).toBe(true)
    const result = (await response.json()) as {
        success: boolean
        data: { id: string }
    }
    expect(result.success).toBe(true)
    expect(result.data.id).toBeTruthy()
    return result.data.id
}

/** 财务详情以准确销售行展示成交价，完整销售版本和真实合同仅在采购授权上下文内读取。 */
export async function expectPurchaseSalesContext(
    page: Page,
    token: string,
    options: {
        purchaseOrderId: string
        salesOrderId: string
        salesOrderNo: string
        salesRevisionId: string
        customerName: string
        contractNo: string
        skuName: string
        salesUnitPrice: string
        salesQuantity: string
        salesGrossTotal: string
        contractBytes: Buffer
        contractAssetId: string
        unrelatedAssetId: string
    },
): Promise<PurchaseSalesContext> {
    const center = await apiGet<PurchaseSalesContext>(
        token,
        `/admin/purchase-orders/${options.purchaseOrderId}`,
    )
    const source = center.source_sales_order
    expect(source, "采购详情必须返回准确来源销售版本").toBeTruthy()
    expect(source!.sales_order_id).toBe(options.salesOrderId)
    expect(source!.sales_order_no).toBe(options.salesOrderNo)
    expect(source!.revision_id).toBe(options.salesRevisionId)
    expect(source!.customer_name).toBe(options.customerName)
    expect(source!.contract_no).toBe(options.contractNo)
    expect(source!.revision_no).toBe(1)
    expect(source!.materials_unavailable).toBe(false)
    expect(source!.lines).toHaveLength(1)
    expect(source!.lines[0]!.quantity).toMatch(
        decimalPattern(options.salesQuantity),
    )
    expect(source!.lines[0]!.gross_amount).toMatch(
        decimalPattern(options.salesGrossTotal),
    )
    expect(source!.totals.gross).toMatch(
        decimalPattern(options.salesGrossTotal),
    )
    const purchaseLines = center.lines.filter(
        (item) => item.line_type === "ITEM_SERVICE",
    )
    expect(
        purchaseLines,
        "采购商品行必须存在，不能跳过成交价关联核验",
    ).toHaveLength(1)
    for (const line of purchaseLines) {
        const sales = source!.lines.find(
            (item) =>
                item.sales_order_revision_line_id ===
                line.sales_order_revision_line_id,
        )
        expect(sales, "采购来源行必须精确匹配销售版本行").toBeTruthy()
        expect(sales!.unit_price_gross).toMatch(
            decimalPattern(options.salesUnitPrice),
        )
        expect(line.unit_cost_gross).not.toMatch(
            decimalPattern(options.salesUnitPrice),
        )
    }
    const file = source!.materials.find((item) => item.kind === "CONTRACT")
    expect(file?.file_asset_id).toBe(options.contractAssetId)
    const denied = await fetch(
        `${API_BASE}/admin/purchase-orders/${center.id}/sales-materials/${options.unrelatedAssetId}/download`,
        {
            headers: { Authorization: `Bearer ${token}` },
            signal: AbortSignal.timeout(15_000),
        },
    )
    expect([403, 404], "未关联文件不得借当前采购下载").toContain(denied.status)

    await page.goto(`/procurement/orders/${center.id}`)
    await page.getByRole("tab", { name: /^概览/ }).click()
    const purchaseTable = page.getByRole("table", { name: "采购明细" })
    await expect(
        purchaseTable.locator('th[data-column-id="salesUnitPrice"]'),
    ).toBeVisible(VISIBLE)
    await expect(
        purchaseTable.locator('th[data-column-id="salesUnitPrice"]'),
    ).toContainText("客户成交含税单价")
    const purchaseRow = purchaseTable
        .getByRole("row")
        .filter({ hasText: options.skuName })
        .first()
    await expect(
        purchaseRow.locator(
            'td[data-column-id="salesUnitPrice"] [data-slot="money-value-number"]',
        ),
    ).toHaveText(displayedCurrency(options.salesUnitPrice))
    expect(purchaseLines[0]!.unit_cost_gross).toBeTruthy()
    await expect(
        purchaseRow.locator(
            'td[data-column-id="unitCost"] [data-slot="money-value-number"]',
        ),
    ).toHaveText(displayedCurrency(purchaseLines[0]!.unit_cost_gross!))
    const prefix = `procurement-orders-source-sales-${idSegment(center.id)}`
    await page.locator(`#${prefix}-open`).click()
    const sheet = page.getByRole("dialog", {
        name: options.customerName,
        exact: true,
    })
    await expect(sheet).toBeVisible(VISIBLE)
    await expect(sheet).toContainText(`销售单号：${options.salesOrderNo}`)
    await expect(sheet).toContainText(
        `采购关联销售版本 v${source!.revision_no}`,
    )
    await expect(sheet).toContainText(options.contractNo)
    await expect(sheet).toContainText(options.salesGrossTotal)
    const salesTable = sheet.locator(`#${prefix}-lines-table`)
    await expect(salesTable.locator("tbody tr")).toHaveCount(
        source!.lines.length,
    )
    await expect(
        salesTable
            .getByRole("row")
            .filter({ hasText: options.skuName })
            .first()
            .locator(
                'td[data-column-id="price"] [data-slot="money-value-number"]',
            ),
    ).toHaveText(displayedCurrency(options.salesUnitPrice))
    const materialPrefix = `${prefix}-material-contract-${idSegment(file!.file_asset_id)}`
    const downloadPath = `/admin/purchase-orders/${center.id}/sales-materials/${file!.file_asset_id}/download`
    const reading = page.waitForResponse(
        (response) =>
            response.request().method() === "GET" &&
            new URL(response.url()).pathname === downloadPath,
    )
    await sheet.locator(`#${materialPrefix}-view`).click()
    const previewResponse = await reading
    await expectMaterialHeaders(previewResponse, options.contractBytes)
    expect(previewResponse.headers()["content-type"]).toBe("application/pdf")
    await expectPresentedBlob(
        sheet.locator(`#${materialPrefix}-preview-document`),
        options.contractBytes,
        false,
    )
    const [download, downloadedResponse] = await Promise.all([
        page.waitForEvent("download"),
        page.waitForResponse(
            (response) =>
                response.request().method() === "GET" &&
                new URL(response.url()).pathname === downloadPath,
        ),
        sheet.locator(`#${materialPrefix}-download`).click(),
    ])
    await expectMaterialHeaders(downloadedResponse, options.contractBytes)
    expect(await download.failure()).toBeNull()
    expect(download.suggestedFilename()).toBe(file!.file_name)
    const downloadedPath = await download.path()
    expect(await fs.readFile(downloadedPath!)).toEqual(options.contractBytes)
    await sheet.locator(`#${prefix}-footer-close`).click()
    return center
}
