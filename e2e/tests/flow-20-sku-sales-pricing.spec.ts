/**
 * 流程: [flow-20] 公司 SKU 四价、数量取价与销售成交快照。
 * 合同: docs/erp-phase-1.md §4.4.2。
 * 账号: caigou（维护公司 SKU）→ xiaoshou（商品池、草稿、提交）→ caigou（审批）。
 * 独立商品通过真实 API 建档，复用固定分类、品牌、单位和有效供应商；
 * 四价维护、商品池查询、销售数量/手动价格/恢复自动及审批均通过页面办理。
 * 供应商成本与公司销售参考价使用不同数值，验证改公司价不改供给成本。
 */
import { archiveContractViaUi } from "../helpers/contracts"
import fs from "node:fs/promises"
import path from "node:path"

import {
    test,
    expect,
    type Locator,
    type Page,
    type Response,
} from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
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
const PRICES = {
    factory_price_gross: "77.00",
    sales_visible_price_gross: "129.00",
    bulk_price_gross: "119.00",
    bulk_min_quantity: "10",
    market_price: "159.00",
}
const PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=",
    "base64",
)

type ApiPage<T> = { items: T[]; total: number }
type CatalogIdentity = {
    id: string
    category_code?: string
    brand_code?: string
    unit_code?: string
    supplier_no?: string
}
type Prices = typeof PRICES
type Sku = { id: string; sku_no: string; current_revision_id: string }
type SellableSku = Prices & {
    sku_id: string
    sku_revision_id: string
    sku_no: string
    supplier_codes: string[]
    supplier_count: number
}
type SalesLine = {
    sku_id: string
    sku_revision_id: string
    quantity: string
    unit_price_gross: string
    pricing_mode: "AUTO" | "MANUAL"
    gross_amount: string
    reference_prices?: Prices
}
type SalesOrder = {
    id: string
    order_no: string
    commercial_status: string
    current_revision_id?: string
    working_copy?: { version: number; gross_amount: string; lines: SalesLine[] }
    submissions: Array<{
        id: string
        submission_no: number
        gross_amount: string
        lines: SalesLine[]
    }>
    revisions: Array<{
        id: string
        gross_amount: string
        content_hash: string
        lines: Array<{ gross_amount: string }>
    }>
}
type Receivable = {
    id: string
    sales_order_id: string
    gross_total: string
    open_total: string
    open_invoiceable_total: string
    current_sales_order_revision_id?: string
    entries: Array<{ id: string; amount: string; source_document_id: string }>
}
type DraftBody = {
    version?: number
    contract_id: string
    draft: {
        requested_contract_revision_id: string
        no_contract_terms?: unknown
        lines: Array<{
            goods: { unit_price_gross: string; pricing_mode: string }
        }>
    }
}

function businessDate(offset = 0): string {
    const date = new Date()
    date.setDate(date.getDate() + offset)
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`
}

async function writeApi<T>(
    token: string,
    method: "POST" | "PUT",
    endpoint: string,
    body: unknown,
): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as {
        success?: boolean
        errorMessage?: string
        data: T
    }
    expect(
        response.ok && result.success !== false,
        `${method} ${endpoint}: ${result.errorMessage ?? response.status}`,
    ).toBe(true)
    return result.data
}

async function readUiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as {
        success?: boolean
        errorMessage?: string
        data: T
    }
    expect(
        response.ok() && result.success !== false,
        `${response.request().method()} ${new URL(response.url()).pathname}: ${result.errorMessage ?? response.status()}`,
    ).toBe(true)
    return result.data
}

async function prepareProduct(
    token: string,
    suffix: string,
): Promise<{
    productId: string
    skuId: string
    skuNo: string
    skuName: string
    supplierIds: string[]
}> {
    const [categories, brands, units, suppliers, admins] = await Promise.all([
        apiGet<ApiPage<CatalogIdentity>>(token, "/admin/product-categories", {
            category_code: "TEA",
            page_size: 100,
        }),
        apiGet<ApiPage<CatalogIdentity>>(token, "/admin/product-brands", {
            brand_code: "SF",
            page_size: 100,
        }),
        apiGet<ApiPage<CatalogIdentity>>(token, "/admin/unit-of-measures", {
            unit_code: "HE",
            page_size: 100,
        }),
        apiGet<ApiPage<CatalogIdentity>>(token, "/admin/suppliers", {
            page_size: 100,
        }),
        apiGet<Array<{ id: string; account: string }>>(token, "/admin/admins"),
    ])
    const category = categories.items.find(
        (item) => item.category_code === "TEA",
    )
    const brand = brands.items.find((item) => item.brand_code === "SF")
    const unit = units.items.find((item) => item.unit_code === "HE")
    const owner = admins.find((item) => item.account === "caigou")
    const selectedSuppliers = ["SUP-HZSF", "SUP-DEV-WEEK"].map((code) => {
        const supplier = suppliers.items.find(
            (item) => item.supplier_no === code,
        )
        expect(supplier, `固定种子缺少有效实物供应商 ${code}`).toBeTruthy()
        return supplier!
    })
    expect(
        category && brand && unit && owner,
        "固定种子须包含茶叶分类、狮峰品牌、盒单位和采购维护人",
    ).toBeTruthy()
    const skuNo = `E2E-PRICE-${suffix}`
    const skuName = `E2E 四价礼盒 ${suffix}`
    const created = await writeApi<{ id: string }>(
        token,
        "POST",
        "/admin/products",
        {
            change_reason: "E2E 独立四价验收商品",
            product_no: skuNo,
            product_kind: "PHYSICAL",
            maintainer_user_id: owner!.id,
            name: skuName,
            category_id: category!.id,
            brand_id: brand!.id,
            status: "active",
            effective_from: businessDate(),
            carousel_media: [],
            detail_media: [],
            skus: [
                {
                    sku_no: skuNo,
                    name: skuName,
                    base_unit_id: unit!.id,
                    sales_visible_price_gross: "100.00",
                    spec_entries: [],
                },
            ],
        },
    )
    const skus = await apiGet<ApiPage<Sku>>(
        token,
        `/admin/products/${created.id}/skus`,
        {
            page_size: 100,
        },
    )
    expect(skus.items).toHaveLength(1)
    const sku = skus.items[0]!
    for (const supplier of selectedSuppliers) {
        await writeApi(token, "POST", "/admin/supplier-offerings", {
            sku_id: sku.id,
            supplier_id: supplier.id,
            supplier_product_code: skuNo,
            supplier_sku_code: skuNo,
            source_type: "MANUAL",
            terms: {
                dropship_supply_price_gross: "51.00",
                bulk_supply_price_gross: "47.00",
                input_tax_rate: "0.09",
                bulk_minimum_order_quantity: "6",
                supply_region: ["全国"],
                product_capabilities: [],
                valid_from: businessDate(),
            },
            availability_status: "AVAILABLE",
            available_quantity: "1000",
            change_reason: "E2E 独立供给成本",
            idempotency_key: `${skuNo}-${supplier.id}`,
        })
    }
    await writeApi(
        token,
        "PUT",
        `/admin/products/${created.id}/listing-status`,
        {
            listing_status: "listed",
        },
    )
    return {
        productId: created.id,
        skuId: sku.id,
        skuNo,
        skuName,
        supplierIds: selectedSuppliers.map((item) => item.id),
    }
}

async function maintainPrices(
    page: Page,
    productId: string,
    prices: Prices,
    uploadImage = false,
): Promise<void> {
    await page.goto(`/master-data/products/${productId}`)
    await expect(
        page.getByRole("heading", { name: "规格与 SKU", exact: true }),
    ).toBeVisible(VISIBLE)
    await page.locator("#product-section-sku").scrollIntoViewIfNeeded()
    if (uploadImage) {
        const [chooser] = await Promise.all([
            page.waitForEvent("filechooser"),
            page
                .getByRole("group", { name: /主图/ })
                .first()
                .getByRole("button", { name: "选择主图" })
                .click(),
        ])
        await chooser.setFiles({
            name: "pricing-sku.png",
            mimeType: "image/png",
            buffer: PNG,
        })
    }
    for (const [label, value] of [
        ["出厂价", prices.factory_price_gross],
        ["一件代发价", prices.sales_visible_price_gross],
        ["集采价", prices.bulk_price_gross],
        ["集采起订量", prices.bulk_min_quantity],
        ["市场价", prices.market_price],
    ]) {
        await page.getByLabel(`默认规格 ${label}`, { exact: true }).fill(value!)
    }
    await dismissToasts(page)
    await page.locator("#master-data-product-detail-header-submit").click()
    const dialog = page.getByRole("dialog", { name: "保存更新", exact: true })
    await expect(dialog).toBeVisible(VISIBLE)
    await dialog
        .locator("#master-data-product-detail-effective-reason")
        .fill("E2E 维护四档含税销售参考价")
    const [response] = await Promise.all([
        page.waitForResponse(
            (item) =>
                item.request().method() === "PUT" &&
                new URL(item.url()).pathname.startsWith(
                    `/admin/products/${productId}`,
                ),
        ),
        dialog.locator("#master-data-product-save-confirm").click(),
    ])
    await readUiResult(response)
    await expect(dialog).toBeHidden(VISIBLE)
    await dismissToasts(page)
}

async function poolSku(token: string, skuNo: string): Promise<SellableSku> {
    const result = await apiGet<ApiPage<SellableSku>>(
        token,
        "/admin/sellable-skus",
        {
            q: skuNo,
            page_size: 100,
        },
    )
    expect(result.items).toHaveLength(1)
    return result.items[0]!
}

async function expectPriceCell(
    table: Locator,
    row: Locator,
    label: string,
    value: string,
): Promise<void> {
    const headers = await table.getByRole("columnheader").allTextContents()
    const index = headers.findIndex((text) => text.includes(label))
    expect(index, `公司商品池须展示 ${label}`).toBeGreaterThanOrEqual(0)
    await expect(row.getByRole("cell").nth(index)).toContainText(value)
}

/** 合同派生版本写入 Form 后才保存或提交，续编价格回显本身不代表合同已加载。 */
async function expectContractReady(
    page: Page,
    contractNo: string,
): Promise<void> {
    const section = page.locator(
        'section[aria-labelledby="sales-create-contract-title"]',
    )
    await expect(
        section.getByText(`${contractNo}@v1`, { exact: true }),
    ).toBeVisible(VISIBLE)
    await expect(section.getByText("加载中…", { exact: true })).toHaveCount(
        0,
        VISIBLE,
    )
    await expect(page.locator("#sales-orders-create-contract")).toHaveAttribute(
        "aria-expanded",
        "false",
        VISIBLE,
    )
}

async function saveDraft(
    page: Page,
    contractNo: string,
    salesOrderId?: string,
    expectedContract?: { id: string; revisionId: string },
): Promise<{ order: SalesOrder; payload: DraftBody }> {
    await expectContractReady(page, contractNo)
    await dismissToasts(page)
    const endpoint = salesOrderId
        ? `/admin/sales-orders/${salesOrderId}/working-copy`
        : "/admin/sales-orders"
    const method = salesOrderId ? "PUT" : "POST"
    const [response] = await Promise.all([
        page.waitForResponse(
            (item) =>
                item.request().method() === method &&
                new URL(item.url()).pathname === endpoint,
        ),
        page.locator("#sales-orders-create-save-draft").click(),
    ])
    const result = await readUiResult<{ id?: string }>(response)
    const id = salesOrderId || result.id
    expect(id).toBeTruthy()
    const payload = response.request().postDataJSON() as DraftBody
    expect(payload.draft.requested_contract_revision_id).toEqual(
        expect.any(String),
    )
    expect(payload.draft.requested_contract_revision_id.length).toBeGreaterThan(
        0,
    )
    expect(payload.draft.no_contract_terms).toBeUndefined()
    if (expectedContract) {
        expect(payload.contract_id).toBe(expectedContract.id)
        expect(payload.draft.requested_contract_revision_id).toBe(
            expectedContract.revisionId,
        )
    }
    return {
        order: await apiGet<SalesOrder>(
            await apiToken("xiaoshou"),
            `/admin/sales-orders/${id}`,
        ),
        payload,
    }
}

function expectWorkingPrice(
    order: SalesOrder,
    pricingMode: "AUTO" | "MANUAL",
    quantity: string,
    price: string,
    gross: string,
): void {
    expect(order.working_copy?.lines).toHaveLength(1)
    expect(order.working_copy?.lines[0]).toMatchObject({
        pricing_mode: pricingMode,
        quantity,
        unit_price_gross: price,
        gross_amount: gross,
    })
    expect(order.working_copy?.gross_amount).toBe(gross)
}

test("[flow-20] 四价与供应商编号、数量自动报价、手动保留及销售快照冻结", async ({
    browser,
}, testInfo) => {
    test.setTimeout(10 * 60 * 1000)
    const suffix = Date.now().toString(36).toUpperCase()
    const adminToken = await apiToken("admin")
    const product = await prepareProduct(adminToken, suffix)
    const { page: procurementPage } = await openLoggedInWorkspace(
        browser,
        "caigou",
    )
    await maintainPrices(procurementPage, product.productId, PRICES, true)
    await ensureDefaultProcurementOwner(procurementPage)

    const { page } = await openLoggedInWorkspace(browser, "xiaoshou")
    const salesToken = await apiToken("xiaoshou")
    const selected = await poolSku(salesToken, product.skuNo)
    expect(selected).toMatchObject(PRICES)
    expect(selected.supplier_codes.sort()).toEqual(["SUP-DEV-WEEK", "SUP-HZSF"])
    expect(selected.supplier_count).toBe(2)

    await test.step("公司商品池表格、预览和筛选导出展示四价与供应商业务编号", async () => {
        await page.goto(
            `/master-data/sellable-items?q=${encodeURIComponent(product.skuNo)}&layout=table`,
        )
        await expect(
            page.getByRole("heading", { name: "公司商品池", exact: true }),
        ).toBeVisible(VISIBLE)
        const table = page.locator("#master-data-sellable-items-list-table")
        const row = table.getByRole("row").filter({ hasText: product.skuNo })
        await expect(row).toHaveCount(1)
        for (const [label, value] of [
            ["出厂价（含税）", "77.00"],
            ["一件代发价（含税）", "129.00"],
            ["集采价（含税）", "119.00"],
            ["市场价（含税）", "159.00"],
        ]) {
            await expectPriceCell(table, row, label!, value!)
        }
        await expectPriceCell(table, row, "供应商编号", "SUP-HZSF")
        await expectPriceCell(table, row, "供应商编号", "SUP-DEV-WEEK")
        await row.getByText(product.skuName, { exact: true }).click()
        const preview = page.getByRole("dialog", {
            name: product.skuName,
            exact: true,
        })
        await expect(preview).toBeVisible(VISIBLE)
        for (const [label, value] of [
            ["出厂价", "77.00"],
            ["一件代发价", "129.00"],
            ["集采价", "119.00"],
            ["市场价", "159.00"],
            ["供应商编号", "SUP-HZSF"],
        ]) {
            await expect(
                preview
                    .locator("dt")
                    .filter({ hasText: new RegExp(`^${label}$`) })
                    .locator("xpath=.."),
            ).toContainText(value!)
        }
        await preview
            .getByRole("button", { name: "关闭", exact: true })
            .last()
            .click()
        const [download] = await Promise.all([
            page.waitForEvent("download"),
            page.locator("#master-data-sellable-items-list-export").click(),
        ])
        expect(download.suggestedFilename()).toMatch(/\.csv$/)
        const output = testInfo.outputPath("company-pool-prices.csv")
        await download.saveAs(output)
        const csv = await fs.readFile(output, "utf8")
        for (const value of [
            product.skuNo,
            "出厂价",
            "77.00",
            "一件代发价",
            "129.00",
            "集采价",
            "119.00",
            "市场价",
            "159.00",
            "供应商编号",
            "SUP-HZSF",
            "SUP-DEV-WEEK",
        ]) {
            expect(csv).toContain(value)
        }
    })

    const customerName = `E2E 四价客户 ${suffix}`
    const contractNo = `E2E-PRICE-CONTRACT-${suffix}`
    await createCustomerViaUi(page, {
        legalName: customerName,
        shortName: `四价${suffix}`,
        creditCode: `91110108MA${suffix}`.slice(0, 18).padEnd(18, "0"),
        paymentTermLabel: "货到 15 天",
        contact: { name: "四价验收", phone: "13800138020" },
        address: "北京市朝阳区四价验收路 20 号",
    })
    await page.goto("/sales/contracts")
    await page.locator("#page-actions-action-upload").click()
    await archiveContractViaUi(page, { contractNo, customerName, pdf: path.resolve(process.cwd(), "fixtures/sample-contract.pdf") })

    await page.goto("/sales/orders?mode=create")
    await chooseOption(
        page,
        page.locator("#sales-orders-create-contract"),
        contractNo,
    )
    await expectContractReady(page, contractNo)
    await chooseOption(
        page,
        page.locator("#sales-orders-create-header-welfare-scene"),
        "年节礼包",
    )
    await page.locator("#sales-orders-create-line-items-add").click()
    const picker = page.getByRole("dialog", { name: "添加商品" })
    await expect(picker).toBeVisible(VISIBLE)
    const search = picker.locator(
        "#master-data-list-sellable-list-toolbar-search-input",
    )
    await search.fill(product.skuNo)
    await search.press("Enter")
    await picker
        .getByRole("checkbox", { name: new RegExp(`选择.*${product.skuName}`) })
        .check()
    await picker.locator("#sales-orders-sku-picker-confirm").click()
    await expect(picker).toBeHidden(VISIBLE)
    const quantity = page.getByLabel("数量", { exact: true })
    const price = page.getByLabel("含税成交单价", { exact: true })
    await expect(price).toHaveValue("129.00")
    await test.step("跨集采门槛自动切价，手动修改后数量变化保留成交价", async () => {
        for (const [count, unitPrice] of [
            ["9", "129.00"],
            ["10", "119.00"],
            ["9", "129.00"],
            ["10", "119.00"],
        ]) {
            await quantity.fill(count!)
            await expect(price).toHaveValue(unitPrice!)
        }
        await price.fill("125.00")
        await quantity.fill("20")
        await expect(price).toHaveValue("125.00")
        await expect(
            page.getByText("手动成交价", { exact: true }),
        ).toBeVisible()
    })
    await page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(
        page,
        page.locator("#sales-orders-create-batch-due-date"),
        businessDate(7),
    )
    await page.locator("#sales-orders-create-batch-due-date-apply").click()
    await expectToast(page, "已批量设置交期")
    const firstSaved = await saveDraft(page, contractNo)
    const salesOrderId = firstSaved.order.id
    const frozenContract = {
        id: firstSaved.payload.contract_id,
        revisionId: firstSaved.payload.draft.requested_contract_revision_id,
    }
    expectWorkingPrice(firstSaved.order, "MANUAL", "20", "125.00", "2500.00")

    await test.step("保存续编保留手动模式，显式恢复 AUTO 并由服务端核价", async () => {
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expectContractReady(page, contractNo)
        await expect(quantity).toHaveValue("20")
        await expect(price).toHaveValue("125.00")
        await quantity.fill("9")
        await expect(price).toHaveValue("125.00")
        const manualSaved = await saveDraft(
            page,
            contractNo,
            salesOrderId,
            frozenContract,
        )
        expectWorkingPrice(
            manualSaved.order,
            "MANUAL",
            "9",
            "125.00",
            "1125.00",
        )
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expectContractReady(page, contractNo)
        await expect(quantity).toHaveValue("9")
        await expect(price).toHaveValue("125.00")
        await page
            .getByRole("button", { name: "按数量取价", exact: true })
            .click()
        await expect(price).toHaveValue("129.00")
        await quantity.fill("10")
        await expect(price).toHaveValue("119.00")
        const automaticSaved = await saveDraft(
            page,
            contractNo,
            salesOrderId,
            frozenContract,
        )
        expectWorkingPrice(
            automaticSaved.order,
            "AUTO",
            "10",
            "119.00",
            "1190.00",
        )
        await price.fill("125.00")
        const changedToManual = await saveDraft(
            page,
            contractNo,
            salesOrderId,
            frozenContract,
        )
        expectWorkingPrice(
            changedToManual.order,
            "MANUAL",
            "10",
            "125.00",
            "1250.00",
        )
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expectContractReady(page, contractNo)
        await expect(price).toHaveValue("125.00")
        await page
            .getByRole("button", { name: "按数量取价", exact: true })
            .click()
        await expect(price).toHaveValue("119.00")
        const saved = await saveDraft(
            page,
            contractNo,
            salesOrderId,
            frozenContract,
        )
        expectWorkingPrice(saved.order, "AUTO", "10", "119.00", "1190.00")
        expect(
            saved.order.working_copy?.lines[0]?.reference_prices,
        ).toMatchObject(PRICES)
        const submittedPrice = structuredClone(saved.payload)
        submittedPrice.version = saved.order.working_copy!.version
        submittedPrice.draft.lines[0]!.goods.unit_price_gross = "0.01"
        await writeApi(
            salesToken,
            "PUT",
            `/admin/sales-orders/${salesOrderId}/working-copy`,
            submittedPrice,
        )
        const checked = await apiGet<SalesOrder>(
            salesToken,
            `/admin/sales-orders/${salesOrderId}`,
        )
        expectWorkingPrice(checked, "AUTO", "10", "119.00", "1190.00")
    })

    await test.step("提交与生效保存成交快照，后续公司 SKU 改价不得重算正式金额", async () => {
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expectContractReady(page, contractNo)
        await expect(price).toHaveValue("119.00")
        await quantity.fill("9")
        await expect(price).toHaveValue("129.00")
        await quantity.fill("10")
        await expect(price).toHaveValue("119.00")
        await dismissToasts(page)
        await submitCreatedSalesOrder(page)
        const submitDialog = page.getByRole("dialog", { name: "提交销售单" })
        const [submittedResponse] = await Promise.all([
            page.waitForResponse(
                (item) =>
                    item.request().method() === "POST" &&
                    new URL(item.url()).pathname ===
                        `/admin/sales-orders/${salesOrderId}/submit`,
            ),
            submitDialog
                .locator("#sales-orders-submit-confirm-confirm")
                .click(),
        ])
        await readUiResult(submittedResponse)
        const submitted = await apiGet<SalesOrder>(
            salesToken,
            `/admin/sales-orders/${salesOrderId}`,
        )
        expect(submitted.submissions).toHaveLength(1)
        expect(submitted.submissions[0]).toMatchObject({
            gross_amount: "1190.00",
            lines: [
                expect.objectContaining({
                    sku_revision_id: selected.sku_revision_id,
                    pricing_mode: "AUTO",
                    quantity: "10",
                    unit_price_gross: "119.00",
                    gross_amount: "1190.00",
                }),
            ],
        })
        await openWorkspaceTask(
            procurementPage,
            "销售单审批",
            submitted.order_no,
            "approval",
        )
        await approveCurrentDocument(procurementPage)
        await expect
            .poll(
                async () =>
                    (
                        await apiGet<SalesOrder>(
                            salesToken,
                            `/admin/sales-orders/${salesOrderId}`,
                        )
                    ).commercial_status,
            )
            .toBe("EFFECTIVE")
        const effective = await apiGet<SalesOrder>(
            salesToken,
            `/admin/sales-orders/${salesOrderId}`,
        )
        expect(effective.revisions).toHaveLength(1)
        expect(effective.revisions[0]?.gross_amount).toBe("1190.00")
        const receivables = await apiGet<ApiPage<Receivable>>(
            adminToken,
            "/admin/receivable-accounts",
            { sales_order_id: salesOrderId, page_size: 100 },
        )
        expect(receivables.items).toHaveLength(1)
        expect(receivables.items[0]).toMatchObject({
            sales_order_id: salesOrderId,
            gross_total: "1190.00",
            open_total: "1190.00",
            open_invoiceable_total: "1190.00",
        })
        await maintainPrices(procurementPage, product.productId, {
            factory_price_gross: "98.00",
            sales_visible_price_gross: "149.00",
            bulk_price_gross: "99.00",
            bulk_min_quantity: "3",
            market_price: "179.00",
        })
        const changedPool = await poolSku(salesToken, product.skuNo)
        expect(changedPool.sku_revision_id).not.toBe(selected.sku_revision_id)
        expect(changedPool.bulk_price_gross).toBe("99.00")
        const unchanged = await apiGet<SalesOrder>(
            salesToken,
            `/admin/sales-orders/${salesOrderId}`,
        )
        expect(unchanged.current_revision_id).toBe(
            effective.current_revision_id,
        )
        expect(unchanged.submissions).toEqual(effective.submissions)
        expect(unchanged.revisions).toEqual(effective.revisions)
        const unchangedReceivables = await apiGet<ApiPage<Receivable>>(
            adminToken,
            "/admin/receivable-accounts",
            { sales_order_id: salesOrderId, page_size: 100 },
        )
        expect(unchangedReceivables.items).toEqual(receivables.items)
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(
            page.getByRole("heading", { name: customerName, exact: true }),
        ).toBeVisible(VISIBLE)
        await expect(
            page.getByText("已生效", { exact: true }).first(),
        ).toBeVisible(VISIBLE)
    })

    const offerings = await apiGet<
        ApiPage<{
            supplier_id: string
            dropship_supply_price_gross: string
            bulk_supply_price_gross: string
            bulk_minimum_order_quantity: string
        }>
    >(adminToken, "/admin/supplier-offerings", {
        sku_id: product.skuId,
        page_size: 100,
    })
    expect(offerings.items).toHaveLength(2)
    expect(offerings.items.map((item) => item.supplier_id).sort()).toEqual(
        product.supplierIds.sort(),
    )
    for (const offering of offerings.items) {
        expect(offering).toMatchObject({
            dropship_supply_price_gross: "51.00",
            bulk_supply_price_gross: "47.00",
            bulk_minimum_order_quantity: "6",
        })
    }
})
