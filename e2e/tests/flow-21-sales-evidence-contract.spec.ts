/**
 * 流程: [flow-21] 客户信用代码可空、无合同销售凭证、受限采购审批资料与后补合同。
 * 合同: docs/erp-phase-1.md §7.3.6。
 * 账号: xiaoshou（客户、合同、销售）→ caigou（采购责任和销售审批）。
 * 全部业务写入走真实页面或已授权 API；凭证使用真实 PDF/PNG 文件，禁止 mock。
 */
import { archiveContractViaUi, SIGNING_COMPANY } from "../helpers/contracts"
import fs from "node:fs/promises"
import path from "node:path"

import { test, expect, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { loginViaUi, openLoggedInWorkspace } from "../helpers/login"
import {
    expectApprovalMaterialPreviewAndDownload,
    expectGenericSalesMaterialReadsDenied,
    openFrozenApprovalMaterials,
    restrictSalesMaterialReader,
    type FrozenApprovalMaterials,
} from "../helpers/purchase-sales-context"
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement"
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
} from "../helpers/ui"

const VISIBLE = { timeout: 20_000 }
const PDF_PATH = path.resolve(process.cwd(), "fixtures/sample-contract.pdf")
const PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=",
    "base64",
)

type Envelope<T> = { success?: boolean; errorMessage?: string; data: T }
type ApiPage<T> = { items: T[]; total: number }
type CustomerProfile = {
    id: string
    party_id: string
    unified_credit_code?: string | null
    current_revision: { id: string; legal_name: string; revision_no: number }
}
type Contract = {
    id: string
    contract_no: string
    customer_id: string
    settlement_party_id: string
    current_revision_id: string
}
type CommercialSnapshot = {
    id: string
    version?: number
    payment_term_code: string
    payment_term_name: string
    invoice_type: string
    tax_point: string
    gross_amount: string
    lines: Array<{
        quantity: string
        unit_price_gross: string
        gross_amount: string
        pricing_mode: string
    }>
}
type SalesOrder = {
    id: string
    order_no: string
    version: number
    customer_id: string
    settlement_party_id: string
    contract_id: string | null
    evidence_file_asset_ids: string[]
    evidence_files: Array<{
        file_asset_id: string
        file_name: string
        content_type: string
        byte_size: number
    }>
    commercial_status: string
    current_revision_id: string | null
    working_copy: CommercialSnapshot | null
    submissions: CommercialSnapshot[]
    revisions: CommercialSnapshot[]
}
type CreateBody = {
    order_no: string
    business_type: string
    contract_id: null
    customer_id: string
    idempotency_key: string
    intent: string
    evidence_file_asset_ids: string[]
    draft: {
        requested_contract_revision_id: null
        no_contract_terms: {
            settlement_party_id: string
            payment_term_code: string
            payment_term_name: string
            invoice_type: string
            tax_point: string
        }
        [key: string]: unknown
    }
}

async function readUiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as Envelope<T>
    expect(
        response.ok() && result.success !== false,
        `${response.request().method()} ${new URL(response.url()).pathname}: ${result.errorMessage ?? response.status()}`,
    ).toBe(true)
    return result.data
}

async function sendCommand<T>(
    token: string,
    endpoint: string,
    body: unknown,
): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST",
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<T>
    expect(
        response.ok && result.success !== false,
        `POST ${endpoint}: ${result.errorMessage ?? response.status}`,
    ).toBe(true)
    return result.data
}

async function expectCommandRejected(
    token: string,
    endpoint: string,
    body: unknown,
    reason: RegExp,
): Promise<void> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST",
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<unknown>
    expect(response.ok && result.success !== false).toBe(false)
    expect([400, 409, 422], result.errorMessage).toContain(response.status)
    expect(result.errorMessage).toMatch(reason)
}

function dueDate(): string {
    const date = new Date()
    date.setDate(date.getDate() + 21)
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`
}

async function createCustomer(
    page: Page,
    legalName: string,
): Promise<CustomerProfile> {
    const created = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/customer-profiles",
    )
    await createCustomerViaUi(page, {
        legalName,
        creditCode: "",
        paymentTermLabel: "货到 15 天",
    })
    const result = await readUiResult<{ customer_id: string }>(await created)
    const profile = await apiGet<CustomerProfile>(
        await apiToken("xiaoshou"),
        `/admin/customer-profiles/${result.customer_id}`,
    )
    expect(profile.unified_credit_code ?? "").toBe("")
    expect(profile.current_revision.legal_name).toBe(legalName)
    return profile
}

async function reviseCreditCode(
    page: Page,
    customerId: string,
    creditCode: string,
): Promise<CustomerProfile> {
    await page.goto(`/sales/customers/${customerId}`)
    await page.locator("#customers-detail-overview-edit").click()
    await page.locator("#customers-form-credit-code").fill(creditCode)
    await page
        .locator("#customers-form-change-reason")
        .fill(creditCode ? "E2E 补录信用代码" : "E2E 清除非必填信用代码")
    const [response] = await Promise.all([
        page.waitForResponse(
            (item) =>
                item.request().method() === "PUT" &&
                new URL(item.url()).pathname ===
                    `/admin/customer-profiles/${customerId}`,
        ),
        page.locator("#customers-form-submit").click(),
    ])
    await readUiResult(response)
    const profile = await apiGet<CustomerProfile>(
        await apiToken("xiaoshou"),
        `/admin/customer-profiles/${customerId}`,
    )
    expect(profile.unified_credit_code ?? "").toBe(creditCode)
    return profile
}

async function uploadContract(
    page: Page,
    token: string,
    customerName: string,
    contractNo: string,
    paymentTerms = "货到 15 天",
): Promise<Contract> {
    await page.goto("/sales/contracts")
    await page.locator("#page-actions-action-upload").click()
    await archiveContractViaUi(page, { contractNo, customerName, pdf: PDF_PATH, paymentTerms })
    const contracts = await apiGet<ApiPage<Contract>>(
        token,
        "/admin/contracts",
        {
            contract_no: contractNo,
            page_size: 100,
        },
    )
    expect(contracts.items).toHaveLength(1)
    return contracts.items[0]!
}

async function createNoContractDraft(
    page: Page,
    token: string,
    customerName: string,
    evidence: "pdf" | "png",
): Promise<{ order: SalesOrder; body: CreateBody }> {
    await page.goto("/sales/orders?mode=create")
    await page.locator("#sales-orders-create-basis-evidence").click()
    await chooseOption(
        page,
        page.locator("#sales-orders-create-customer"),
        customerName,
    )
    await chooseOption(
        page,
        page.locator("#sales-orders-create-settlement"),
        customerName,
    )
    const [uploadedResponse] = await Promise.all([
        page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                new URL(response.url()).pathname ===
                    "/admin/file-assets/upload",
        ),
        page.locator("#sales-orders-create-evidence-input").setInputFiles(
            evidence === "pdf"
                ? PDF_PATH
                : {
                      name: "sales-order-evidence.png",
                      mimeType: "image/png",
                      buffer: PNG,
                  },
        ),
    ])
    const uploaded = await readUiResult<{ id: string }>(uploadedResponse)
    await chooseOption(
        page,
        page.locator("#sales-orders-create-header-welfare-scene"),
        "年节礼包",
    )
    await chooseOption(
        page,
        page.locator("#sales-orders-create-header-payment-terms"),
        "货到 15 天",
    )
    await page.locator("#sales-orders-create-header-tax-rate").fill("13")
    await page.locator("#sales-orders-create-line-items-add").click()
    const picker = page.getByRole("dialog", { name: "添加商品" })
    await expect(picker).toBeVisible(VISIBLE)
    const search = picker.locator(
        "#master-data-list-sellable-list-toolbar-search-input",
    )
    await search.fill("龙井")
    await search.press("Enter")
    await picker
        .getByRole("checkbox", { name: /选择.*狮峰明前龙井礼盒/ })
        .first()
        .check()
    await picker.locator("#sales-orders-sku-picker-confirm").click()
    await expect(picker).toBeHidden(VISIBLE)
    await page.getByLabel("数量", { exact: true }).fill("2")
    await page.getByLabel("含税成交单价", { exact: true }).fill("137.00")
    await page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(
        page,
        page.locator("#sales-orders-create-batch-due-date"),
        dueDate(),
    )
    await page.locator("#sales-orders-create-batch-due-date-apply").click()
    await expectToast(page, "已批量设置交期")
    await dismissToasts(page)
    const [response] = await Promise.all([
        page.waitForResponse(
            (item) =>
                item.request().method() === "POST" &&
                new URL(item.url()).pathname === "/admin/sales-orders",
        ),
        page.locator("#sales-orders-create-save-draft").click(),
    ])
    const created = await readUiResult<{ id: string }>(response)
    const body = response.request().postDataJSON() as CreateBody
    const order = await apiGet<SalesOrder>(
        token,
        `/admin/sales-orders/${created.id}`,
    )
    expect(body.contract_id).toBeNull()
    expect(body.evidence_file_asset_ids).toEqual([uploaded.id])
    expect(body.draft.no_contract_terms).toEqual({
        settlement_party_id: order.settlement_party_id,
        payment_term_code: "POSTPAY_NET15",
        payment_term_name: "货到 15 天",
        invoice_type: "增值税专用发票",
        tax_point: "13",
    })
    expect(order).toMatchObject({
        commercial_status: "DRAFT",
        contract_id: null,
        evidence_file_asset_ids: [uploaded.id],
    })
    expect(order.working_copy).toMatchObject({
        payment_term_code: "POSTPAY_NET15",
        tax_point: "13",
        gross_amount: "274.00",
        lines: [
            expect.objectContaining({
                quantity: "2",
                unit_price_gross: "137.00",
                pricing_mode: "MANUAL",
            }),
        ],
    })
    expect(order.evidence_files).toHaveLength(1)
    expect(order.evidence_files[0]).toMatchObject({
        file_asset_id: uploaded.id,
        content_type: evidence === "pdf" ? "application/pdf" : "image/png",
    })
    return { order, body }
}

async function supplementContract(
    page: Page,
    token: string,
    original: SalesOrder,
    contract: Contract,
): Promise<SalesOrder> {
    if (original.commercial_status === "DRAFT") {
        // 草稿对象中心进入续编表单；独立补录命令必须仍保留已保存的商业内容。
        await sendCommand(
            token,
            `/admin/sales-orders/${original.id}/contract`,
            {
                version: original.version,
                contract_id: contract.id,
                requested_contract_revision_id: contract.current_revision_id,
            },
        )
    } else {
        await page.goto(`/sales/orders/${original.id}?section=overview`)
        await page.locator("#sales-order-overview-supplement-contract").click()
        const dialog = page.getByRole("dialog", { name: "补录销售合同" })
        await expect(dialog).toBeVisible(VISIBLE)
        await chooseOption(
            page,
            dialog.locator("#sales-order-contract-supplement-select"),
            contract.contract_no,
        )
        await expect(
            dialog.locator("#sales-order-contract-supplement-submit"),
        ).toBeEnabled(VISIBLE)
        const [response] = await Promise.all([
            page.waitForResponse(
                (item) =>
                    item.request().method() === "POST" &&
                    new URL(item.url()).pathname ===
                        `/admin/sales-orders/${original.id}/contract`,
            ),
            dialog.locator("#sales-order-contract-supplement-submit").click(),
        ])
        await readUiResult(response)
        await expect(dialog).toBeHidden(VISIBLE)
        await expect(
            page.locator("#sales-order-overview-supplement-contract"),
        ).toHaveCount(0)
    }
    const bound = await apiGet<SalesOrder>(
        token,
        `/admin/sales-orders/${original.id}`,
    )
    expect(bound).toMatchObject({
        id: original.id,
        order_no: original.order_no,
        customer_id: original.customer_id,
        settlement_party_id: original.settlement_party_id,
        contract_id: contract.id,
        commercial_status: original.commercial_status,
        current_revision_id: original.current_revision_id,
        evidence_file_asset_ids: original.evidence_file_asset_ids,
    })
    if (original.commercial_status === "DRAFT" && original.working_copy) {
        // 绑定以工作副本版本守卫与并发保存互斥，只递增版本，不改商务内容。
        expect(original.working_copy.version).toBeDefined()
        expect(bound.working_copy).toEqual({
            ...original.working_copy,
            version: original.working_copy.version! + 1,
        })
    } else {
        expect(bound.working_copy).toEqual(original.working_copy)
    }
    expect(bound.submissions).toEqual(original.submissions)
    expect(bound.revisions).toEqual(original.revisions)
    return bound
}

async function expectContractFilter(
    page: Page,
    token: string,
    customer: CustomerProfile,
    hasContract: boolean,
    included: SalesOrder,
    excluded: SalesOrder,
): Promise<void> {
    await page.goto(
        `/sales/orders?q=${encodeURIComponent(customer.current_revision.legal_name)}&layout=table`,
    )
    await page.locator("#sales-orders-list-filter-more-toggle").click()
    await chooseOption(
        page,
        page.locator("#sales-orders-list-filter-has-contract"),
        hasContract ? "有合同" : "无合同",
    )
    // 面板内“应用”提交全部筛选并关闭浮层；直接关闭会撤销未应用的条件。
    await page.locator("#sales-orders-list-filter-more-apply").click()
    await expect(page.locator("#sales-orders-list-filter-panel")).toBeHidden()
    await expect(page).toHaveURL(
        new RegExp(`hasContract=${hasContract ? "yes" : "no"}`),
    )
    const table = page.locator("#sales-orders-list-table")
    await expect(
        table.getByRole("row").filter({ hasText: included.order_no }),
    ).toHaveCount(1)
    await expect(
        table.getByRole("row").filter({ hasText: excluded.order_no }),
    ).toHaveCount(0)
    await expect(
        page.getByText(`合同状态：${hasContract ? "有合同" : "无合同"}`, {
            exact: true,
        }),
    ).toBeVisible()
    const filtered = await apiGet<ApiPage<SalesOrder>>(
        token,
        "/admin/sales-orders",
        {
            customer_id: customer.id,
            has_contract: hasContract,
            q: customer.current_revision.legal_name,
            page_size: 100,
        },
    )
    expect(filtered.items.map((item) => item.id)).toEqual([included.id])
}

test("[flow-21] 信用代码可空、无合同凭证采购审批、合同筛选与原单补合同", async ({
    browser,
}, testInfo) => {
    test.setTimeout(10 * 60 * 1000)
    const suffix = Date.now().toString(36).toUpperCase()
    const { page: procurementPage } = await openLoggedInWorkspace(
        browser,
        "caigou",
    )
    await ensureDefaultProcurementOwner(procurementPage)
    const { page } = await openLoggedInWorkspace(browser, "xiaoshou")
    const token = await apiToken("xiaoshou")
    let customer!: CustomerProfile
    let otherCustomer!: CustomerProfile

    await test.step("客户信用代码允许留空、后补并再次清除", async () => {
        customer = await createCustomer(page, `E2E 无合同客户 ${suffix}`)
        const creditCode = `91${suffix}E2ECREDIT`.slice(0, 18).padEnd(18, "0")
        const filled = await reviseCreditCode(page, customer.id, creditCode)
        customer = await reviseCreditCode(page, customer.id, "")
        expect(customer.current_revision.revision_no).toBe(
            filled.current_revision.revision_no + 1,
        )
        otherCustomer = await createCustomer(page, `E2E 异客户合同 ${suffix}`)
    })

    const firstContract = await uploadContract(
        page,
        token,
        customer.current_revision.legal_name,
        `E2E-EVIDENCE-${suffix}-1`,
    )
    const replacementContract = await uploadContract(
        page,
        token,
        customer.current_revision.legal_name,
        `E2E-EVIDENCE-${suffix}-2`,
        "先款 100%",
    )
    const foreignContract = await uploadContract(
        page,
        token,
        otherCustomer.current_revision.legal_name,
        `E2E-EVIDENCE-${suffix}-OTHER`,
    )
    const companies = await apiGet<ApiPage<{ id: string; legal_name: string }>>(
        token, "/admin/companies", { page_size: 100 },
    )
    const signingCompany = companies.items.find((company) => company.legal_name === SIGNING_COMPANY)
    expect(signingCompany).toBeDefined()
    expect(firstContract.settlement_party_id).toBe(signingCompany!.id)

    const pdfDraft = await createNoContractDraft(
        page,
        token,
        customer.current_revision.legal_name,
        "pdf",
    )
    const imageDraft = await createNoContractDraft(
        page,
        token,
        customer.current_revision.legal_name,
        "png",
    )
    expect(pdfDraft.order.customer_id).toBe(customer.id)
    expect(imageDraft.order.customer_id).toBe(customer.id)
    expect(pdfDraft.order.settlement_party_id).toBe(customer.party_id)
    expect(imageDraft.order.settlement_party_id).toBe(customer.party_id)

    await test.step("服务端拒绝无凭证开单，已上传凭证可从原销售单授权下载", async () => {
        const invalid = structuredClone(pdfDraft.body)
        invalid.order_no = `E2E-MISSING-EVIDENCE-${suffix}`
        invalid.idempotency_key = `missing-evidence-${suffix}`
        invalid.evidence_file_asset_ids = []
        await expectCommandRejected(
            token,
            "/admin/sales-orders",
            invalid,
            /无合同.*上传.*凭证/,
        )
        const rejectedOrders = await apiGet<ApiPage<SalesOrder>>(
            token,
            "/admin/sales-orders",
            {
                q: invalid.order_no,
            },
        )
        expect(rejectedOrders.items).toHaveLength(0)
        for (const { order, expectedBytes } of [
            {
                order: pdfDraft.order,
                expectedBytes: await fs.readFile(PDF_PATH),
            },
            { order: imageDraft.order, expectedBytes: PNG },
        ]) {
            const assetId = order.evidence_file_asset_ids[0]!
            const downloaded = await fetch(
                `${API_BASE}/admin/sales-orders/${order.id}/evidence-files/${assetId}/download`,
                {
                    headers: { Authorization: `Bearer ${token}` },
                    signal: AbortSignal.timeout(20_000),
                },
            )
            expect(downloaded.ok).toBe(true)
            expect(Buffer.from(await downloaded.arrayBuffer())).toEqual(
                expectedBytes,
            )
            expect(order.evidence_files[0]?.byte_size).toBe(
                expectedBytes.byteLength,
            )
        }
    })

    await test.step("不同客户或条款不一致的合同不得绑定，匹配合同保留原单条款与价格", async () => {
        await expectCommandRejected(
            token,
            `/admin/sales-orders/${pdfDraft.order.id}/contract`,
            {
                version: pdfDraft.order.version,
                contract_id: foreignContract.id,
                requested_contract_revision_id:
                    foreignContract.current_revision_id,
            },
            /客户.*合同对方主体.*客户一致/,
        )
        await expectCommandRejected(
            token,
            `/admin/sales-orders/${pdfDraft.order.id}/contract`,
            {
                version: pdfDraft.order.version,
                contract_id: replacementContract.id,
                requested_contract_revision_id: replacementContract.current_revision_id,
            },
            /合同条款不一致.*付款条件/,
        )
        expect(
            await apiGet<SalesOrder>(
                token,
                `/admin/sales-orders/${pdfDraft.order.id}`,
            ),
        ).toEqual(pdfDraft.order)
        pdfDraft.order = await supplementContract(
            page,
            token,
            pdfDraft.order,
            firstContract,
        )
        await expectCommandRejected(
            token,
            `/admin/sales-orders/${pdfDraft.order.id}/contract`,
            {
                version: pdfDraft.order.version,
                contract_id: replacementContract.id,
                requested_contract_revision_id:
                    replacementContract.current_revision_id,
            },
            /已有关联合同.*替换/,
        )
        expect(
            await apiGet<SalesOrder>(
                token,
                `/admin/sales-orders/${pdfDraft.order.id}`,
            ),
        ).toEqual(pdfDraft.order)
    })

    await test.step("销售列表合同筛选与客户关键字共同生效", async () => {
        await expectContractFilter(
            page,
            token,
            customer,
            true,
            pdfDraft.order,
            imageDraft.order,
        )
        await expectContractFilter(
            page,
            token,
            customer,
            false,
            imageDraft.order,
            pdfDraft.order,
        )
    })

    await test.step("采购无销售、合同或通用文件权限仍可核对销售审批凭证，后补合同不得回填", async () => {
        await page.goto(`/sales/orders/${imageDraft.order.id}`)
        await submitCreatedSalesOrder(page)
        const dialog = page.getByRole("dialog", { name: "提交销售单" })
        const [submittedResponse] = await Promise.all([
            page.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    new URL(response.url()).pathname ===
                        `/admin/sales-orders/${imageDraft.order.id}/submit`,
            ),
            dialog.locator("#sales-orders-submit-confirm-confirm").click(),
        ])
        await readUiResult(submittedResponse)
        const submitted = await apiGet<SalesOrder>(
            token,
            `/admin/sales-orders/${imageDraft.order.id}`,
        )
        expect(submitted.contract_id).toBeNull()
        expect(submitted.submissions).toHaveLength(1)
        // 续编页面把税率回显为两位小数；提交快照冻结本次实际输入的 13.00。
        expect(submitted.submissions[0]).toMatchObject({
            gross_amount: "274.00",
            payment_term_code: "POSTPAY_NET15",
            tax_point: "13.00",
        })
        const evidenceId = imageDraft.order.evidence_file_asset_ids[0]!
        let originalApproval!: {
            instanceId: string
            materials: FrozenApprovalMaterials
        }
        const restoreRole = await restrictSalesMaterialReader(
            "caigou",
            `FLOW21-${suffix}`,
        )
        try {
            // 角色变更后用独立上下文重新登录，页面和 API 使用本次受限资格。
            const restrictedContext = await browser.newContext()
            try {
                const restrictedPage = await restrictedContext.newPage()
                await loginViaUi(restrictedPage, "caigou")
                const procurementToken = await apiToken("caigou")
                await expectGenericSalesMaterialReadsDenied(procurementToken, {
                    salesOrderId: submitted.id,
                    contractId: firstContract.id,
                    fileAssetId: evidenceId,
                })
                await openWorkspaceTask(
                    restrictedPage,
                    "销售单审批",
                    submitted.order_no,
                    "approval",
                )
                originalApproval =
                    await openFrozenApprovalMaterials(restrictedPage)
                expect(originalApproval.materials).toMatchObject({
                    document_type: "sales_order",
                    document_id: submitted.id,
                    subject_version: 1,
                    display: {
                        source: {
                            customer: customer.current_revision.legal_name,
                            amount_label: "¥274",
                            lines: [
                                expect.objectContaining({
                                    title: expect.stringContaining("龙井"),
                                    quantity:
                                        expect.stringMatching(/^2\s+.*¥274$/),
                                }),
                            ],
                        },
                    },
                    attachments: [
                        {
                            file_asset_id: evidenceId,
                            file_name: "sales-order-evidence.png",
                            content_type: "image/png",
                            byte_size: PNG.byteLength,
                        },
                    ],
                })
                const materialsDialog = restrictedPage.getByRole("dialog", {
                    name: /提交资料$/,
                })
                await expect(
                    materialsDialog.getByRole("heading", {
                        name: "审批提交资料",
                        exact: true,
                    }),
                ).toBeVisible(VISIBLE)
                await expect(materialsDialog).toContainText("龙井")
                await expect(materialsDialog).toContainText("¥274")
                await expect(materialsDialog).toContainText(
                    "sales-order-evidence.png",
                )
                await expect(materialsDialog).not.toContainText(
                    firstContract.contract_no,
                )
                await expectApprovalMaterialPreviewAndDownload(restrictedPage, {
                    instanceId: originalApproval.instanceId,
                    file: originalApproval.materials.attachments[0]!,
                    bytes: PNG,
                })
                const foreignFile = await restrictedPage.request.get(
                    `${API_BASE}/admin/approval-instances/${originalApproval.instanceId}/materials/${pdfDraft.order.evidence_file_asset_ids[0]!}/download`,
                    {
                        headers: {
                            Authorization: `Bearer ${procurementToken}`,
                        },
                    },
                )
                expect(
                    foreignFile.status(),
                    "当前审批实例不得读取另一销售单上传的凭证",
                ).toBe(404)
                await restrictedPage
                    .locator("#workspace-document-paper-dialog-close")
                    .click()
                await approveCurrentDocument(restrictedPage)
            } finally {
                await restrictedContext.close()
            }
        } finally {
            await restoreRole()
        }
        await expect
            .poll(
                async () =>
                    (
                        await apiGet<SalesOrder>(
                            token,
                            `/admin/sales-orders/${imageDraft.order.id}`,
                        )
                    ).commercial_status,
            )
            .toBe("EFFECTIVE")
        const effective = await apiGet<SalesOrder>(
            token,
            `/admin/sales-orders/${imageDraft.order.id}`,
        )
        expect(effective.revisions).toHaveLength(1)
        expect(effective.revisions[0]).toMatchObject({
            gross_amount: "274.00",
            payment_term_code: "POSTPAY_NET15",
            tax_point: "13.00",
        })
        imageDraft.order = await supplementContract(
            page,
            token,
            effective,
            firstContract,
        )
        // 原发起人按自己的审批读取资格核对历史材料，禁止用超管代替业务授权。
        const historicalMaterials = await apiGet<FrozenApprovalMaterials>(
            token,
            `/admin/approval-instances/${originalApproval.instanceId}/materials`,
        )
        expect(historicalMaterials).toEqual(originalApproval.materials)
        expect(
            historicalMaterials.attachments.map((file) => file.file_asset_id),
        ).toEqual([evidenceId])
        expect(JSON.stringify(historicalMaterials.display)).not.toContain(
            firstContract.contract_no,
        )
        const supplementedContract = await apiGet<
            Contract & {
                revisions: Array<{
                    id: string
                    contract_pdf_file_id: string
                }>
            }
        >(token, `/admin/contracts/${firstContract.id}`)
        expect(supplementedContract.customer_id).toBe(
            imageDraft.order.customer_id,
        )
        expect(supplementedContract.id).toBe(imageDraft.order.contract_id)
        const supplementedPdf = supplementedContract.revisions.find(
            (revision) => revision.id === firstContract.current_revision_id,
        )?.contract_pdf_file_id
        expect(
            supplementedPdf,
            "后补合同必须保留实际归档 PDF 引用",
        ).toBeTruthy()
        const historicalPath = `${API_BASE}/admin/approval-instances/${originalApproval.instanceId}/materials`
        for (const action of ["preview", "download"] as const) {
            const rejectedSupplement = await page.request.get(
                `${historicalPath}/${supplementedPdf!}/${action}`,
                { headers: { Authorization: `Bearer ${token}` } },
            )
            expect(
                rejectedSupplement.status(),
                `旧审批 ${action} 白名单不得接纳后补合同 PDF`,
            ).toBe(404)
            const originalEvidence = await page.request.get(
                `${historicalPath}/${evidenceId}/${action}`,
                { headers: { Authorization: `Bearer ${token}` } },
            )
            expect(originalEvidence.ok()).toBe(true)
            expect(originalEvidence.headers()["content-type"]).toBe(
                action === "preview" ? "image/png" : "application/octet-stream",
            )
            expect(originalEvidence.headers()["cache-control"]).toContain(
                "no-store",
            )
            expect(await originalEvidence.body()).toEqual(PNG)
        }
    })

    await test.step("作废单不允许补合同且不会改变原单关系", async () => {
        const body = structuredClone(pdfDraft.body)
        body.order_no = `E2E-VOID-EVIDENCE-${suffix}`
        body.idempotency_key = `void-evidence-${suffix}`
        const created = await sendCommand<SalesOrder>(
            token,
            "/admin/sales-orders",
            body,
        )
        const voided = await sendCommand<SalesOrder>(
            token,
            `/admin/sales-orders/${created.id}/void`,
            {
                version: created.version,
            },
        )
        expect(voided.commercial_status).toBe("VOIDED")
        await expectCommandRejected(
            token,
            `/admin/sales-orders/${voided.id}/contract`,
            {
                version: voided.version,
                contract_id: firstContract.id,
                requested_contract_revision_id:
                    firstContract.current_revision_id,
            },
            /作废.*不能补合同/,
        )
        expect(
            await apiGet<SalesOrder>(token, `/admin/sales-orders/${voided.id}`),
        ).toEqual(voided)
        await page.goto(`/sales/orders/${voided.id}?section=overview`)
        await expect(
            page.getByText("已作废", { exact: true }).first(),
        ).toBeVisible(VISIBLE)
        await expect(
            page.locator("#sales-order-overview-supplement-contract"),
        ).toHaveCount(0)
    })

    await testInfo.attach("销售凭证与合同验收记录", {
        body: JSON.stringify(
            {
                customer_id: customer.id,
                draft_sales_order_id: pdfDraft.order.id,
                effective_sales_order_id: imageDraft.order.id,
                contract_id: firstContract.id,
            },
            null,
            2,
        ),
        contentType: "application/json",
    })
})
