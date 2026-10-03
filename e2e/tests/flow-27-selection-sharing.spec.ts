/**
 * [flow-27] 销售选品真实准备、公开选择、销售方案与链接生命周期。
 * 独立商品经真实 API 建档；客户、发起选品、陈列删除、发布、更换链接、
 * 客户保存/核对/提交、单档重生成、撤销与关闭通过实际页面办理。
 * 校验公开白名单、服务端金额、会话冲突、成功回放与冻结方案，不模拟业务响应。
 */
import { test, expect, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import { chooseOption, dismissToasts } from "../helpers/ui"

type ApiPage<T> = { items: T[]; total: number }
type Envelope<T> = { success: boolean; data: T; errorMessage?: string }
type Item = {
    item_id: string
    removed?: boolean
    name: string
    price: string
    cover_path?: string | null
    members: Array<{ sku_id?: string; name: string; price: string }>
}
type Book = {
    id: string
    version: number
    status: string
    batch_id: string
    display_count: number
    removed_count: number
    items: Item[]
    tiers: Array<{ tier_id: string }>
    prepared_at: number
    eligibility_as_of: string
    link_revoked: boolean
    proposal_id?: string
    proposal_no?: string
    last_prepare_failure?: string | null
}
type PublicView = {
    kind: string
    session_version: number
    items: Item[]
    choices: Array<{ item_id: string; quantity?: number | null; line_amount?: string | null }>
    total_amount: string | null
    receipt?: {
        proposal_no: string
        items: Array<{ item_id: string; quantity?: number | null; line_amount?: string | null }>
        total_amount: string | null
    }
}
type Proposal = {
    id: string
    proposal_no: string
    customer_id: string
    booklet_id: string
    sales_owner_user_id: string
    business_org_unit_id: string
    form: string
    submit_mode: string
    total_amount: string | null
    display_lines: Array<{ display_item_id: string; quantity?: number | null; unit_price: string; line_amount?: string | null }>
    sku_lines: Array<{ name: string; quantity?: number | null; unit_price: string; line_amount?: string | null }>
}

async function request<T>(method: string, endpoint: string, body?: unknown, token?: string) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
            ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const envelope = await response.json() as Envelope<T>
    return { response, envelope }
}

async function write<T>(endpoint: string, body: unknown, token?: string, method = "POST"): Promise<T> {
    const result = await request<T>(method, endpoint, body, token)
    expect(result.response.ok && result.envelope.success !== false, `${method} ${endpoint.replace(/\/public\/selection\/[^/]+/, "/public/selection/{token}")}: ${result.envelope.errorMessage ?? result.response.status}`).toBe(true)
    return result.envelope.data
}

async function uiResult<T>(response: Response): Promise<T> {
    const envelope = await response.json() as Envelope<T>
    expect(response.ok() && envelope.success !== false, envelope.errorMessage).toBe(true)
    return envelope.data
}

async function clickWrite<T>(page: Page, selector: string, endpoint: string) {
    const [response] = await Promise.all([
        page.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === endpoint),
        page.locator(selector).click(),
    ])
    return { data: await uiResult<T>(response), body: response.request().postDataJSON() as Record<string, unknown> }
}

async function book(token: string, id: string) {
    return apiGet<Book>(token, `/admin/sales-selection-books/${id}`)
}

async function prepared(token: string, id: string) {
    await expect.poll(async () => {
        const result = await book(token, id)
        expect(result.last_prepare_failure, "真实后台准备任务失败").toBeFalsy()
        return result.status
    }, { timeout: 90_000, intervals: [500, 1000, 2000] }).toBe("PENDING_PUBLISH")
    return book(token, id)
}

function tokenFromPath(publicPath: string) {
    const path = new URL(publicPath, "http://e2e.local").pathname
    expect(path).toMatch(/^\/s\/[^/]+$/)
    return path.split("/").at(-1)!
}

async function publicView(token: string) {
    const result = await request<PublicView>("GET", `/public/selection/${token}`)
    expect(result.response.ok && result.envelope.success !== false).toBe(true)
    return result.envelope.data
}

function assertPublicFields(value: unknown) {
    if (Array.isArray(value)) {
        value.forEach(assertPublicFields)
        return
    }
    if (!value || typeof value !== "object") return
    const forbidden = ["sku_id", "sku_revision_id", "product_id", "supplier_id", "supplier_sku_code", "supplier_codes", "dropship_supply_price_gross", "bulk_supply_price_gross", "factory_price_gross", "bulk_price_gross", "market_price", "cost", "storage_object_key", "sales_owner_user_id", "business_org_unit_id", "customer_id"]
    for (const [key, child] of Object.entries(value)) {
        expect(forbidden, `公开响应不得包含内部字段 ${key}`).not.toContain(key)
        assertPublicFields(child)
    }
}

async function setupCatalog(token: string, prefix: string) {
    const [categories, brands, units, suppliers, admins, organization] = await Promise.all([
        apiGet<ApiPage<{ id: string; category_code: string }>>(token, "/admin/product-categories", { category_code: "TEA", page_size: 100 }),
        apiGet<ApiPage<{ id: string; brand_code: string }>>(token, "/admin/product-brands", { brand_code: "SF", page_size: 100 }),
        apiGet<ApiPage<{ id: string; unit_code: string }>>(token, "/admin/unit-of-measures", { unit_code: "HE", page_size: 100 }),
        apiGet<ApiPage<{ id: string; supplier_no: string }>>(token, "/admin/suppliers", { page_size: 100 }),
        apiGet<Array<{ id: string; account: string; name: string }>>(token, "/admin/admins"),
        apiGet<{ people: Array<{ id: string; account: string; own_org_unit_id: string | null }> }>(token, "/admin/org-units"),
    ])
    const category = categories.items.find((row) => row.category_code === "TEA")!
    const brand = brands.items.find((row) => row.brand_code === "SF")!
    const unit = units.items.find((row) => row.unit_code === "HE")!
    const supplier = suppliers.items.find((row) => row.supplier_no === "SUP-HZSF")!
    const procurement = admins.find((row) => row.account === "caigou")!
    const sales = admins.find((row) => row.account === "xiaoshou")!
    const orgId = organization.people.find((row) => row.account === "xiaoshou")?.own_org_unit_id
    expect(category && brand && unit && supplier && procurement && sales && orgId, "固定种子必须包含商品字典、供给资格和销售主属组织").toBeTruthy()
    const today = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Shanghai" }).format(new Date())
    const imageData = new FormData()
    const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=", "base64")
    imageData.append("file", new Blob([new Uint8Array(png)], { type: "image/png" }), `selection-${prefix}.png`)
    imageData.append("sensitivity_class", "general")
    imageData.append("retention_class", "long_term")
    const uploadResponse = await fetch(`${API_BASE}/admin/file-assets/upload`, { method: "POST", headers: { Authorization: `Bearer ${token}` }, body: imageData, signal: AbortSignal.timeout(20_000) })
    const upload = await uploadResponse.json() as Envelope<{ id: string }>
    expect(uploadResponse.ok && upload.success, upload.errorMessage).toBe(true)
    const skus: Array<{ id: string; name: string; price: string }> = []
    for (const [index, price] of ["25.00", "35.00", "45.00"].entries()) {
        const code = `${prefix}-${index + 1}`
        const name = `选品礼盒 ${code}`
        const product = await write<{ id: string }>("/admin/products", {
            product_no: code,
            product_kind: "PHYSICAL",
            maintainer_user_id: procurement.id,
            name,
            category_id: category.id,
            brand_id: brand.id,
            status: "active",
            effective_from: today,
            change_reason: "E2E 独立选品商品",
            carousel_media: [],
            detail_media: [],
            skus: [{ sku_no: code, name, base_unit_id: unit.id, main_image_asset_id: upload.data.id, sales_visible_price_gross: price, factory_price_gross: "11.00", bulk_price_gross: "20.00", bulk_min_quantity: "10", market_price: "99.00", spec_entries: [] }],
        }, token)
        const skuPage = await apiGet<ApiPage<{ id: string }>>(token, `/admin/products/${product.id}/skus`, { page_size: 100 })
        expect(skuPage.items).toHaveLength(1)
        const sku = skuPage.items[0]!
        await write("/admin/supplier-offerings", {
            sku_id: sku.id,
            supplier_id: supplier.id,
            supplier_sku_code: code,
            source_type: "MANUAL",
            terms: { dropship_supply_price_gross: "8.00", bulk_supply_price_gross: "7.00", input_tax_rate: "0.09", bulk_minimum_order_quantity: "1", supply_region: ["全国"], product_capabilities: [], valid_from: today },
            availability_status: "AVAILABLE",
            available_quantity: "1000",
            change_reason: "E2E 独立选品供给",
            idempotency_key: `selection-supply-${code}`,
        }, token)
        await write(`/admin/products/${product.id}/listing-status`, { listing_status: "listed" }, token, "PUT")
        skus.push({ id: sku.id, name, price })
    }
    return { skus, sales, orgId: orgId! }
}

test("[flow-27] 选品册准备、客户提交冻结方案、商城套餐与分享链接管理", async ({ browser }) => {
    test.setTimeout(300_000)
    const suffix = `${Date.now()}`
    const prefix = `E2E-SELECT-${suffix}`
    const adminToken = await apiToken("admin")
    const fixture = await setupCatalog(adminToken, prefix)
    const { page: salesPage } = await openLoggedInWorkspace(browser, "xiaoshou")
    const salesToken = await apiToken("xiaoshou")
    const customerName = `E2E 选品客户 ${suffix}`
    await createCustomerViaUi(salesPage, { legalName: customerName, shortName: `选品${suffix}`, paymentTermLabel: "货到 15 天" })
    const customers = await apiGet<ApiPage<{ id: string; legal_name: string }>>(salesToken, "/admin/customers", { keyword: customerName, page_size: 100 })
    const customer = customers.items.find((row) => row.legal_name === customerName)!
    expect(customer).toBeTruthy()
    let selectedBook!: Book
    let selected!: Item
    let publicPath = ""
    let publicToken = ""
    const publicContext = await browser.newContext({ viewport: { width: 390, height: 844 } })
    const customerPage = await publicContext.newPage()

    await test.step("商品池发起单品选品并由真实后台任务冻结陈列，创建重试不重复建册", async () => {
        await salesPage.goto(`/master-data/sellable-items?q=${encodeURIComponent(prefix)}`)
        await expect(salesPage.getByText(fixture.skus[0]!.name, { exact: true }).first()).toBeVisible({ timeout: 20_000 })
        await salesPage.locator("#master-data-sellable-items-launch-selection").click()
        const dialog = salesPage.getByRole("dialog", { name: "发起选品", exact: true })
        const customerInput = dialog.locator("#sales-selection-create-customer")
        await customerInput.click()
        // 防抖搜索会撤下初始列表；等待当前查询返回后，再由通用 helper 点选稳定候选。
        const [customerSearchResponse] = await Promise.all([
            salesPage.waitForResponse((response) => {
                const url = new URL(response.url())
                return response.request().method() === "GET" && url.pathname === "/admin/customers" && url.searchParams.get("keyword") === customerName
            }),
            customerInput.fill(customerName),
        ])
        const candidatePage = await uiResult<ApiPage<{ id: string }>>(customerSearchResponse)
        expect(candidatePage.items.map((candidate) => candidate.id)).toContain(customer.id)
        await expect(customerInput).toHaveAttribute("aria-busy", "false")
        await chooseOption(salesPage, customerInput, customerName, customerName)
        await chooseOption(salesPage, dialog.locator("#sales-selection-create-owner"), fixture.sales.name)
        await dialog.locator("#sales-selection-create-org").fill(fixture.orgId)
        const created = await clickWrite<Book>(salesPage, "#sales-selection-create-submit", "/admin/sales-selection-books")
        expect(created.body).toMatchObject({ customer_id: customer.id, sales_owner_user_id: fixture.sales.id, business_org_unit_id: fixture.orgId })
        expect(created.data.status).toBe("PREPARING")
        const replay = await write<Book>("/admin/sales-selection-books", created.body, salesToken)
        expect(replay.id).toBe(created.data.id)
        selectedBook = await prepared(salesToken, created.data.id)
        expect(selectedBook.items.map((row) => row.name).sort()).toEqual(fixture.skus.map((row) => row.name).sort())
        expect(selectedBook.display_count).toBe(3)
        const list = await apiGet<{ page: ApiPage<Book> }>(salesToken, "/admin/sales-selection-books", { customer_id: customer.id, page_size: 100 })
        expect(list.page.items.map((row) => row.id)).toEqual([selectedBook.id])
    })

    await test.step("删除陈列只影响本册，发布必须确认当前准备批次", async () => {
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        const removed = selectedBook.items.find((row) => row.price === "45.00")!
        const [response] = await Promise.all([
            salesPage.waitForResponse((candidate) => candidate.request().method() === "DELETE" && new URL(candidate.url()).pathname.endsWith(`/display-items/${removed.item_id}`)),
            salesPage.locator(`[id^="sales-selection-preview-"][id$="-delete"]`).filter({ hasText: "删除该项" }).nth(selectedBook.items.indexOf(removed)).click(),
        ])
        selectedBook = await uiResult<Book>(response)
        expect(selectedBook.display_count).toBe(2)
        expect(selectedBook.removed_count).toBe(1)
        expect(selectedBook.items.find((row) => row.item_id === removed.item_id)?.removed).toBe(true)
        const rejected = await request<Book>("POST", `/admin/sales-selection-books/${selectedBook.id}/publish`, { expected_version: selectedBook.version, idempotency_key: `missing-batch-${suffix}` }, salesToken)
        expect(rejected.envelope.success).toBe(false)
        expect(rejected.envelope.errorMessage).toContain("准备批次")
        expect((await book(salesToken, selectedBook.id)).status).toBe("PENDING_PUBLISH")
        await dismissToasts(salesPage)
        const published = await clickWrite<Book>(salesPage, "#sales-selection-detail-publish", `/admin/sales-selection-books/${selectedBook.id}/publish`)
        expect(published.body.batch_id).toBe(selectedBook.batch_id)
        expect(published.data.status).toBe("PUBLISHED")
        selectedBook = published.data
        const link = await apiGet<{ public_url: string }>(salesToken, `/admin/sales-selection-books/${selectedBook.id}/link`)
        publicPath = link.public_url
        publicToken = tokenFromPath(publicPath)
        const view = await publicView(publicToken)
        expect(view.kind).toBe("SELECTING")
        expect(view.items).toHaveLength(2)
        assertPublicFields(view)
        selected = view.items.find((row) => row.price === "25.00")!
        expect(selected.cover_path, "准备任务必须保存实际 S3 图片快照").toBeTruthy()
        const image = await fetch(`${API_BASE}/public/selection/${publicToken}/images?ref=${encodeURIComponent(selected.cover_path!)}`)
        expect(image.status).toBe(200)
        expect(image.headers.get("cache-control")).toBe("private, no-store")
        expect(Buffer.from(await image.arrayBuffer()).byteLength).toBeGreaterThan(0)
        await expect(salesPage.locator('[id^="sales-selection-preview-"][id$="-delete"]')).toHaveCount(0)
    })

    await test.step("移动端保存选择由服务端计价，非法项与过期会话不得覆盖已存选择", async () => {
        await customerPage.goto(publicPath)
        await expect(customerPage.getByText(selected.name, { exact: true }).first()).toBeVisible({ timeout: 20_000 })
        await customerPage.locator(`label[for="sales-selection-public-select-${selected.item_id}"]`).click()
        await customerPage.locator(`#sales-selection-public-qty-${selected.item_id}`).fill("3")
        const saved = await clickWrite<PublicView>(customerPage, "#sales-selection-public-save", `/public/selection/${publicToken}/session`)
        expect(saved.data.total_amount).toBe("75.00")
        expect(saved.data.choices).toEqual([{ item_id: selected.item_id, quantity: 3, line_amount: "75.00" }])
        const stale = await request<PublicView>("POST", `/public/selection/${publicToken}/session`, { expected_session_version: saved.data.session_version - 1, idempotency_key: `stale-session-${suffix}`, choices: [{ item_id: selected.item_id, quantity: 9 }] })
        expect(stale.response.status).toBe(409)
        const foreign = await request<PublicView>("POST", `/public/selection/${publicToken}/session`, { expected_session_version: saved.data.session_version, idempotency_key: `foreign-choice-${suffix}`, choices: [{ item_id: "e2e-other-book-item", quantity: 1 }] })
        expect(foreign.envelope.success).toBe(false)
        for (const quantity of [0, 100001]) {
            const invalid = await request<PublicView>("POST", `/public/selection/${publicToken}/session`, { expected_session_version: saved.data.session_version, idempotency_key: `quantity-${quantity}-${suffix}`, choices: [{ item_id: selected.item_id, quantity }] })
            expect(invalid.envelope.success, "份数必须在 1 至 100000 之间").toBe(false)
        }
        const duplicate = await request<PublicView>("POST", `/public/selection/${publicToken}/session`, { expected_session_version: saved.data.session_version, idempotency_key: `duplicate-choice-${suffix}`, choices: [{ item_id: selected.item_id, quantity: 1 }, { item_id: selected.item_id, quantity: 2 }] })
        expect(duplicate.envelope.success, "同一陈列项不得重复保存").toBe(false)
        expect((await publicView(publicToken)).choices).toEqual(saved.data.choices)
        expect((await publicView(publicToken)).total_amount).toBe("75.00")
        expect((await publicView(publicToken)).session_version).toBe(saved.data.session_version)
        await customerPage.reload()
        await expect(customerPage.locator(`#sales-selection-public-qty-${selected.item_id}`)).toHaveValue("3")
    })

    await test.step("销售更换链接后旧令牌结束，新令牌保留会话与陈列", async () => {
        const previousToken = publicToken
        const previous = await publicView(previousToken)
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        await clickWrite<Book>(salesPage, "#sales-selection-detail-replace-link", `/admin/sales-selection-books/${selectedBook.id}/replace-link`)
        const link = await apiGet<{ public_url: string }>(salesToken, `/admin/sales-selection-books/${selectedBook.id}/link`)
        publicPath = link.public_url
        publicToken = tokenFromPath(publicPath)
        expect(publicToken).not.toBe(previousToken)
        const oldLink = await request<PublicView>("GET", `/public/selection/${previousToken}`)
        expect(oldLink.response.status, "更换后旧令牌不得读取本册").toBe(404)
        expect(oldLink.envelope.success).toBe(false)
        await customerPage.reload()
        await expect(customerPage.getByRole("heading", { name: "选品链接已失效", exact: true })).toBeVisible({ timeout: 20_000 })
        const rotated = await publicView(publicToken)
        expect(rotated.items).toEqual(previous.items)
        expect(rotated.choices).toEqual(previous.choices)
        expect(rotated.session_version).toBe(previous.session_version)
        await customerPage.goto(publicPath)
        await expect(customerPage.locator(`#sales-selection-public-qty-${selected.item_id}`)).toHaveValue("3")
    })

    await test.step("客户核对提交生成唯一冻结方案；重复提交与后续保存只读回执", async () => {
        await clickWrite<PublicView>(customerPage, "#sales-selection-public-review", `/public/selection/${publicToken}/session`)
        const submitted = await clickWrite<PublicView>(customerPage, "#sales-selection-public-submit", `/public/selection/${publicToken}/submit`)
        expect(submitted.data.kind).toBe("RECEIPT")
        expect(submitted.data.receipt?.total_amount).toBe("75.00")
        await expect(customerPage.getByRole("heading", { name: "已提交选品", exact: true })).toBeVisible({ timeout: 20_000 })
        const replay = await write<PublicView>(`/public/selection/${publicToken}/submit`, submitted.body)
        expect(replay.receipt).toEqual(submitted.data.receipt)
        selectedBook = await book(salesToken, selectedBook.id)
        expect(selectedBook.status).toBe("SUBMITTED")
        const proposal = await apiGet<Proposal>(salesToken, `/admin/sales-selection-proposals/${selectedBook.proposal_id}`)
        expect(proposal).toMatchObject({ customer_id: customer.id, booklet_id: selectedBook.id, sales_owner_user_id: fixture.sales.id, business_org_unit_id: fixture.orgId, form: "SINGLE_SKU", submit_mode: "BY_QUANTITY", total_amount: "75.00" })
        expect(proposal.display_lines).toEqual([expect.objectContaining({ display_item_id: selected.item_id, quantity: 3, unit_price: "25.00", line_amount: "75.00" })])
        expect(proposal.sku_lines).toEqual([expect.objectContaining({ name: selected.name, quantity: 3, unit_price: "25.00", line_amount: "75.00" })])
        const proposals = await apiGet<{ page: ApiPage<Proposal> }>(salesToken, "/admin/sales-selection-proposals", { booklet_id: selectedBook.id, page_size: 100 })
        expect(proposals.page.items.map((row) => row.id)).toEqual([proposal.id])
        const orders = await apiGet<ApiPage<{ id: string }>>(salesToken, "/admin/sales-orders", { customer_id: customer.id, page_size: 100 })
        expect(orders.items, "客户确认选品方案不得直接创建销售单").toEqual([])
        const receivables = await apiGet<ApiPage<{ id: string }>>(adminToken, "/admin/receivable-accounts", { customer_id: customer.id, page_size: 100 })
        expect(receivables.items, "客户确认选品方案不得产生应收").toEqual([])
        const lateSave = await write<PublicView>(`/public/selection/${publicToken}/session`, { expected_session_version: 0, idempotency_key: `late-save-${suffix}`, choices: [{ item_id: selected.item_id, quantity: 99 }] })
        expect(lateSave.kind).toBe("RECEIPT")
        expect(lateSave.receipt).toEqual(submitted.data.receipt)
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        await salesPage.locator("#sales-selection-detail-open-proposal").click()
        await expect(salesPage.getByRole("heading", { name: proposal.proposal_no, exact: true })).toBeVisible({ timeout: 20_000 })
        await expect(salesPage.getByText(selected.name, { exact: true })).toBeVisible()
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        await clickWrite<Book>(salesPage, "#sales-selection-detail-revoke", `/admin/sales-selection-books/${selectedBook.id}/revoke-link`)
        expect((await publicView(publicToken)).kind).toBe("ENDED")
        expect(await apiGet<Proposal>(salesToken, `/admin/sales-selection-proposals/${proposal.id}`)).toEqual(proposal)
        expect(await book(salesToken, selectedBook.id)).toMatchObject({ status: "SUBMITTED", proposal_id: proposal.id, link_revoked: true })
        const revokedImage = await fetch(`${API_BASE}/public/selection/${publicToken}/images?ref=${encodeURIComponent(selected.cover_path!)}`)
        expect(revokedImage.ok, "撤销链接后图片读取资格同时失效").toBe(false)
        await customerPage.reload()
        await expect(customerPage.getByRole("heading", { name: "选品已结束", exact: true })).toBeVisible({ timeout: 20_000 })
    })

    await test.step("套餐单档重生成保留冻结商品池，商城兑换只确认款式且无数量金额", async () => {
        const createInput = { customer_id: customer.id, sales_owner_user_id: fixture.sales.id, business_org_unit_id: fixture.orgId, form: "PACKAGE", submit_mode: "MALL_REDEEM", pool_source_kind: "SELECTION", sku_ids: fixture.skus.map((row) => row.id), tiers: [{ name: "60 元档", target_amount: "60.00", tolerance: "0.00", expected_count: 1, sku_count: 2 }], idempotency_key: `package-book-${suffix}` }
        const created = await write<Book>("/admin/sales-selection-books", createInput, salesToken)
        let packageBook = await prepared(salesToken, created.id)
        expect(packageBook.display_count).toBe(1)
        expect(packageBook.items[0]).toMatchObject({ price: "60.00", members: expect.arrayContaining([expect.objectContaining({ price: "25.00" }), expect.objectContaining({ price: "35.00" })]) })
        const frozen = { preparedAt: packageBook.prepared_at, eligibilityAsOf: packageBook.eligibility_as_of, batch: packageBook.batch_id, itemIds: packageBook.items.map((item) => item.item_id) }
        await salesPage.goto(`/sales/selection/${created.id}`)
        await clickWrite<Book>(salesPage, `#selection-regenerate-inline-${packageBook.tiers[0]!.tier_id}`, `/admin/sales-selection-books/${created.id}/prepare`)
        packageBook = await prepared(salesToken, created.id)
        expect(packageBook.batch_id, "重生成沿用冻结批次").toBe(frozen.batch)
        expect(packageBook.items.map((item) => item.item_id), "重生成成功替换该档陈列").not.toEqual(frozen.itemIds)
        expect(packageBook.prepared_at).toBe(frozen.preparedAt)
        expect(packageBook.eligibility_as_of).toBe(frozen.eligibilityAsOf)
        await salesPage.reload()
        await clickWrite<Book>(salesPage, "#sales-selection-detail-publish", `/admin/sales-selection-books/${created.id}/publish`)
        const link = await apiGet<{ public_url: string }>(salesToken, `/admin/sales-selection-books/${created.id}/link`)
        const mallToken = tokenFromPath(link.public_url)
        await customerPage.goto(link.public_url)
        const mallView = await publicView(mallToken)
        assertPublicFields(mallView)
        const mallItem = mallView.items[0]!
        await customerPage.locator(`label[for="sales-selection-public-select-${mallItem.item_id}"]`).click()
        await expect(customerPage.locator('[id^="sales-selection-public-qty-"]')).toHaveCount(0)
        const confirmed = await clickWrite<PublicView>(customerPage, "#sales-selection-public-review", `/public/selection/${mallToken}/session`)
        expect(confirmed.data.total_amount).toBeNull()
        expect(confirmed.data.choices).toEqual([{ item_id: mallItem.item_id, quantity: null, line_amount: null }])
        const withQuantity = await request<PublicView>("POST", `/public/selection/${mallToken}/session`, { expected_session_version: confirmed.data.session_version, idempotency_key: `mall-quantity-${suffix}`, choices: [{ item_id: mallItem.item_id, quantity: 1 }] })
        expect(withQuantity.envelope.success, "商城兑换只确认款式，不接受采购数量").toBe(false)
        expect((await publicView(mallToken)).session_version).toBe(confirmed.data.session_version)
        const submitted = await clickWrite<PublicView>(customerPage, "#sales-selection-public-submit", `/public/selection/${mallToken}/submit`)
        expect(submitted.data.receipt?.total_amount).toBeNull()
        const submittedBook = await book(salesToken, created.id)
        const proposal = await apiGet<Proposal>(salesToken, `/admin/sales-selection-proposals/${submittedBook.proposal_id}`)
        expect(proposal).toMatchObject({ form: "PACKAGE", submit_mode: "MALL_REDEEM", total_amount: null })
        expect(proposal.sku_lines).toHaveLength(2)
        for (const line of [...proposal.sku_lines, ...proposal.display_lines]) {
            expect(line.quantity).toBeNull()
            expect(line.line_amount).toBeNull()
        }
    })

    await test.step("未提交册可关闭，待发布册可作废；终止后不得进入客户选品", async () => {
        const base = { customer_id: customer.id, sales_owner_user_id: fixture.sales.id, business_org_unit_id: fixture.orgId, form: "SINGLE_SKU", submit_mode: "BY_QUANTITY", pool_source_kind: "SELECTION", sku_ids: [fixture.skus[0]!.id] }
        const created = await write<Book>("/admin/sales-selection-books", { ...base, idempotency_key: `close-book-${suffix}` }, salesToken)
        await prepared(salesToken, created.id)
        await salesPage.goto(`/sales/selection/${created.id}`)
        await clickWrite<Book>(salesPage, "#sales-selection-detail-publish", `/admin/sales-selection-books/${created.id}/publish`)
        const link = await apiGet<{ public_url: string }>(salesToken, `/admin/sales-selection-books/${created.id}/link`)
        const closed = await clickWrite<Book>(salesPage, "#sales-selection-detail-close", `/admin/sales-selection-books/${created.id}/close`)
        expect(closed.data.status).toBe("CLOSED")
        expect((await publicView(tokenFromPath(link.public_url))).kind).toBe("ENDED")
        const draft = await write<Book>("/admin/sales-selection-books", { ...base, idempotency_key: `void-book-${suffix}` }, salesToken)
        await prepared(salesToken, draft.id)
        await salesPage.goto(`/sales/selection/${draft.id}`)
        const voided = await clickWrite<Book>(salesPage, "#sales-selection-detail-void", `/admin/sales-selection-books/${draft.id}/void`)
        expect(voided.data.status).toBe("VOIDED")
        const noLink = await request("GET", `/admin/sales-selection-books/${draft.id}/link`, undefined, salesToken)
        expect(noLink.envelope.success).toBe(false)
    })
    await publicContext.close()
})
