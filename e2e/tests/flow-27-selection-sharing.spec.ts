/**
 * [flow-27] 销售选品真实准备、公开选择、销售方案与链接生命周期。
 * 独立商品经真实 API 建档；客户、发起选品、陈列删除、发布、更换链接、
 * 客户保存/核对/提交、单档重生成、撤销与关闭通过实际页面办理。
 * 校验密码保护、独立提货额度、个人地址、公开白名单、服务端金额、
 * 会话冲突、成功回放、冻结方案与真实导出，不模拟业务响应。
 */
import { test, expect, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import { chooseOption, dismissToasts } from "../helpers/ui"
import { readImportWorkbook } from "../helpers/import-workbook"
import { prepareLegacySelectionBook } from "../helpers/selection-legacy"

test.beforeEach(() => {
    expect(process.env.ERP_E2E_ISOLATED, "选品流程必须在隔离 shard 执行").toBe(
        "1",
    )
    expect(
        process.env.ERP_E2E_CONFIG_PATH,
        "选品流程必须绑定当前 shard 配置",
    ).toBeTruthy()
})

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
    per_person_budget?: string | null
}
type Recipient = {
    name: string
    phone: string
    province: string
    city: string
    district: string
    address: string
}
type Voucher = {
    voucher_code: string
    participant_id: string
    submitted: boolean
    proposal_id?: string | null
}
type SelectionDetail = {
    participant_id: string
    voucher_code: string
    proposal_id: string
    proposal_no: string
    recipient: Recipient
    total_amount: string
    items: Proposal["sku_lines"]
}
type PublicView = {
    kind: string
    participant_id?: string | null
    session_version: number
    items: Item[]
    choices: Array<{
        item_id: string
        quantity?: number | null
        line_amount?: string | null
    }>
    total_amount: string | null
    per_person_budget?: string | null
    recipient?: Recipient | null
    receipt?: {
        proposal_no: string
        items: Array<{
            item_id: string
            quantity?: number | null
            line_amount?: string | null
        }>
        total_amount: string | null
    }
}
type ProposalListRow = {
    id: string
    proposal_no: string
    booklet_id: string
    participant_id: string
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
    participant_id?: string | null
    recipient?: Recipient | null
    display_lines: Array<{
        display_item_id: string
        quantity?: number | null
        unit_price: string
        line_amount?: string | null
    }>
    sku_lines: Array<{
        display_item_id: string
        name: string
        quantity?: number | null
        unit_price: string
        line_amount?: string | null
    }>
}

async function request<T>(
    method: string,
    endpoint: string,
    body?: unknown,
    token?: string,
    accessToken?: string,
) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
            ...(accessToken ? { "X-Selection-Access": accessToken } : {}),
            ...(body === undefined
                ? {}
                : { "Content-Type": "application/json" }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const envelope = (await response.json()) as Envelope<T>
    return { response, envelope }
}

async function write<T>(
    endpoint: string,
    body: unknown,
    token?: string,
    method = "POST",
    accessToken?: string,
): Promise<T> {
    const result = await request<T>(method, endpoint, body, token, accessToken)
    expect(
        result.response.ok && result.envelope.success !== false,
        `${method} ${endpoint.replace(/\/public\/selection\/[^/]+/, "/public/selection/{token}")}: ${result.envelope.errorMessage ?? result.response.status}`,
    ).toBe(true)
    return result.envelope.data
}

async function uiResult<T>(response: Response): Promise<T> {
    const envelope = (await response.json()) as Envelope<T>
    expect(
        response.ok() && envelope.success !== false,
        envelope.errorMessage,
    ).toBe(true)
    return envelope.data
}

async function clickWrite<T>(page: Page, selector: string, endpoint: string) {
    const [response] = await Promise.all([
        page.waitForResponse(
            (candidate) =>
                candidate.request().method() === "POST" &&
                new URL(candidate.url()).pathname === endpoint,
        ),
        page.locator(selector).click(),
    ])
    return {
        data: await uiResult<T>(response),
        body: response.request().postDataJSON() as Record<string, unknown>,
    }
}

async function book(token: string, id: string) {
    return apiGet<Book>(token, `/admin/sales-selection-books/${id}`)
}

async function prepared(token: string, id: string) {
    await expect
        .poll(
            async () => {
                const result = await book(token, id)
                expect(
                    result.last_prepare_failure,
                    "真实后台准备任务失败",
                ).toBeFalsy()
                return result.status
            },
            { timeout: 90_000, intervals: [500, 1000, 2000] },
        )
        .toBe("PENDING_PUBLISH")
    return book(token, id)
}

function tokenFromPath(publicPath: string) {
    const path = new URL(publicPath, "http://e2e.local").pathname
    expect(path).toMatch(/^\/s\/[^/]+$/)
    return path.split("/").at(-1)!
}

async function publicView(token: string, accessToken?: string) {
    const result = await request<PublicView>(
        "GET",
        `/public/selection/${token}`,
        undefined,
        undefined,
        accessToken,
    )
    expect(result.response.ok && result.envelope.success !== false).toBe(true)
    return result.envelope.data
}

async function unlock(token: string, password: string, voucherCode?: string) {
    return write<{ access_token: string; page: PublicView }>(
        `/public/selection/${token}/unlock`,
        {
            password,
            ...(voucherCode ? { voucher_code: voucherCode } : {}),
        },
    )
}

async function unlockViaUi(
    page: Page,
    token: string,
    password: string,
    voucherCode?: string,
) {
    await page.locator("#sales-selection-public-password").fill(password)
    if (voucherCode)
        await page
            .locator("#sales-selection-public-voucher-code")
            .fill(voucherCode)
    const result = await clickWrite<{ access_token: string; page: PublicView }>(
        page,
        "#sales-selection-public-unlock",
        `/public/selection/${token}/unlock`,
    )
    expect(result.data.access_token).toBeTruthy()
    assertPublicFields(result.data.page)
    return result.data
}

async function assertLocked(
    token: string,
    item: Item,
    staleAccessToken?: string,
) {
    const locked = await publicView(token, staleAccessToken)
    expect(locked.kind).toBe("LOCKED")
    expect(locked.items).toEqual([])
    expect(locked.choices).toEqual([])
    expect(locked.receipt).toBeFalsy()
    expect(locked.recipient).toBeFalsy()
    expect(locked.total_amount).toBeNull()
    assertPublicFields(locked)
    const save = await request<PublicView>(
        "POST",
        `/public/selection/${token}/session`,
        {
            expected_session_version: 0,
            idempotency_key: `unauthorized-save-${Date.now()}`,
            choices: [{ item_id: item.item_id, quantity: 1 }],
        },
        undefined,
        staleAccessToken,
    )
    expect(
        save.response.ok && save.envelope.success !== false,
        "未解锁不得保存选品",
    ).toBe(false)
    const submit = await request<PublicView>(
        "POST",
        `/public/selection/${token}/submit`,
        {
            expected_session_version: 0,
            idempotency_key: `unauthorized-submit-${Date.now()}`,
        },
        undefined,
        staleAccessToken,
    )
    expect(
        submit.response.ok && submit.envelope.success !== false,
        "未解锁不得生成方案",
    ).toBe(false)
    const image = await fetch(
        `${API_BASE}/public/selection/${token}/images?ref=${encodeURIComponent(item.cover_path!)}${staleAccessToken ? `&access_token=${encodeURIComponent(staleAccessToken)}` : ""}`,
    )
    expect(image.ok, "未解锁不得读取商品图片").toBe(false)
}

async function fillRecipient(page: Page, recipient: Recipient) {
    for (const [key, value] of Object.entries(recipient)) {
        await page
            .locator(`#sales-selection-public-recipient-${key}`)
            .fill(value)
    }
}

async function assertMobileWidth(page: Page) {
    expect(page.viewportSize()?.width).toBe(390)
    await expect
        .poll(
            () =>
                page.evaluate(
                    () =>
                        document.documentElement.scrollWidth -
                        window.innerWidth,
                ),
            {
                message: "390px 移动端页面不得出现横向溢出",
                timeout: 5_000,
            },
        )
        .toBeLessThanOrEqual(0)
}

function assertPublicFields(value: unknown) {
    if (Array.isArray(value)) {
        value.forEach(assertPublicFields)
        return
    }
    if (!value || typeof value !== "object") return
    const forbidden = [
        "sku_id",
        "sku_revision_id",
        "product_id",
        "supplier_id",
        "supplier_sku_code",
        "supplier_codes",
        "dropship_supply_price_gross",
        "bulk_supply_price_gross",
        "factory_price_gross",
        "bulk_price_gross",
        "market_price",
        "cost",
        "storage_object_key",
        "sales_owner_user_id",
        "business_org_unit_id",
        "customer_id",
    ]
    for (const [key, child] of Object.entries(value)) {
        expect(forbidden, `公开响应不得包含内部字段 ${key}`).not.toContain(key)
        assertPublicFields(child)
    }
}

async function setupCatalog(token: string, prefix: string) {
    const [categories, brands, units, suppliers, admins, organization] =
        await Promise.all([
            apiGet<ApiPage<{ id: string; category_code: string }>>(
                token,
                "/admin/product-categories",
                {
                    category_code: "TEA",
                    page_size: 100,
                },
            ),
            apiGet<ApiPage<{ id: string; brand_code: string }>>(
                token,
                "/admin/product-brands",
                {
                    brand_code: "SF",
                    page_size: 100,
                },
            ),
            apiGet<ApiPage<{ id: string; unit_code: string }>>(
                token,
                "/admin/unit-of-measures",
                {
                    unit_code: "HE",
                    page_size: 100,
                },
            ),
            apiGet<ApiPage<{ id: string; supplier_no: string }>>(
                token,
                "/admin/suppliers",
                {
                    page_size: 100,
                },
            ),
            apiGet<Array<{ id: string; account: string; name: string }>>(
                token,
                "/admin/admins",
            ),
            apiGet<{
                people: Array<{
                    id: string
                    account: string
                    own_org_unit_id: string | null
                }>
            }>(token, "/admin/org-units"),
        ])
    const category = categories.items.find(
        (row) => row.category_code === "TEA",
    )!
    const brand = brands.items.find((row) => row.brand_code === "SF")!
    const unit = units.items.find((row) => row.unit_code === "HE")!
    const supplier = suppliers.items.find(
        (row) => row.supplier_no === "SUP-HZSF",
    )!
    const procurement = admins.find((row) => row.account === "caigou")!
    const sales = admins.find((row) => row.account === "xiaoshou")!
    const orgId = organization.people.find(
        (row) => row.account === "xiaoshou",
    )?.own_org_unit_id
    expect(
        category && brand && unit && supplier && procurement && sales && orgId,
        "固定种子必须包含商品字典、供给资格和销售主属组织",
    ).toBeTruthy()
    const today = new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai",
    }).format(new Date())
    const imageData = new FormData()
    const png = Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=",
        "base64",
    )
    imageData.append(
        "file",
        new Blob([new Uint8Array(png)], { type: "image/png" }),
        `selection-${prefix}.png`,
    )
    imageData.append("sensitivity_class", "general")
    imageData.append("retention_class", "long_term")
    const uploadResponse = await fetch(`${API_BASE}/admin/file-assets/upload`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}` },
        body: imageData,
        signal: AbortSignal.timeout(20_000),
    })
    const upload = (await uploadResponse.json()) as Envelope<{ id: string }>
    expect(uploadResponse.ok && upload.success, upload.errorMessage).toBe(true)
    const skus: Array<{ id: string; name: string; price: string }> = []
    for (const [index, price] of ["25.00", "35.00", "45.00"].entries()) {
        const code = `${prefix}-${index + 1}`
        const name = `选品礼盒 ${code}`
        const product = await write<{ id: string }>(
            "/admin/products",
            {
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
                skus: [
                    {
                        sku_no: code,
                        name,
                        base_unit_id: unit.id,
                        main_image_asset_id: upload.data.id,
                        sales_visible_price_gross: price,
                        factory_price_gross: "11.00",
                        bulk_price_gross: "20.00",
                        bulk_min_quantity: "10",
                        market_price: "99.00",
                        spec_entries: [],
                    },
                ],
            },
            token,
        )
        const skuPage = await apiGet<ApiPage<{ id: string }>>(
            token,
            `/admin/products/${product.id}/skus`,
            { page_size: 100 },
        )
        expect(skuPage.items).toHaveLength(1)
        const sku = skuPage.items[0]!
        await write(
            "/admin/supplier-offerings",
            {
                sku_id: sku.id,
                supplier_id: supplier.id,
                supplier_sku_code: code,
                source_type: "MANUAL",
                terms: {
                    dropship_supply_price_gross: "8.00",
                    bulk_supply_price_gross: "7.00",
                    input_tax_rate: "0.09",
                    bulk_minimum_order_quantity: "1",
                    supply_region: ["全国"],
                    product_capabilities: [],
                    valid_from: today,
                },
                availability_status: "AVAILABLE",
                available_quantity: "1000",
                change_reason: "E2E 独立选品供给",
                idempotency_key: `selection-supply-${code}`,
            },
            token,
        )
        await write(
            `/admin/products/${product.id}/listing-status`,
            { listing_status: "listed" },
            token,
            "PUT",
        )
        skus.push({ id: sku.id, name, price })
    }
    return { skus, sales, orgId: orgId! }
}

test("[flow-27] 选品册准备、客户提交冻结方案、商城套餐与分享链接管理", async ({
    browser,
}) => {
    test.setTimeout(300_000)
    const suffix = `${Date.now()}`
    const prefix = `E2E-SELECT-${suffix}`
    const accessPassword = `Selection-${suffix}`
    const adminToken = await apiToken("admin")
    const fixture = await setupCatalog(adminToken, prefix)
    const { page: salesPage } = await openLoggedInWorkspace(browser, "xiaoshou")
    const salesToken = await apiToken("xiaoshou")
    const customerName = `E2E 选品客户 ${suffix}`
    await createCustomerViaUi(salesPage, {
        legalName: customerName,
        shortName: `选品${suffix}`,
        paymentTermLabel: "货到 15 天",
    })
    const customers = await apiGet<ApiPage<{ id: string; legal_name: string }>>(
        salesToken,
        "/admin/customers",
        { keyword: customerName, page_size: 100 },
    )
    const customer = customers.items.find(
        (row) => row.legal_name === customerName,
    )!
    expect(customer).toBeTruthy()
    let selectedBook!: Book
    let selected!: Item
    let publicPath = ""
    let publicToken = ""
    let accessToken = ""
    let savedBody: Record<string, unknown> = {}
    const publicContext = await browser.newContext({
        viewport: { width: 390, height: 844 },
    })
    const customerPage = await publicContext.newPage()

    await test.step("商品池发起单品选品并由真实后台任务冻结陈列，创建重试不重复建册", async () => {
        await salesPage.goto(
            `/master-data/sellable-items?q=${encodeURIComponent(prefix)}`,
        )
        await expect(
            salesPage.getByText(fixture.skus[0]!.name, { exact: true }).first(),
        ).toBeVisible({
            timeout: 20_000,
        })
        await salesPage
            .locator("#master-data-sellable-items-launch-selection")
            .click()
        const dialog = salesPage.getByRole("dialog", {
            name: "发起选品",
            exact: true,
        })
        const customerInput = dialog.locator("#sales-selection-create-customer")
        await customerInput.click()
        // 防抖搜索会撤下初始列表；等待当前查询返回后，再由通用 helper 点选稳定候选。
        const [customerSearchResponse] = await Promise.all([
            salesPage.waitForResponse((response) => {
                const url = new URL(response.url())
                return (
                    response.request().method() === "GET" &&
                    url.pathname === "/admin/customers" &&
                    url.searchParams.get("keyword") === customerName
                )
            }),
            customerInput.fill(customerName),
        ])
        const candidatePage = await uiResult<ApiPage<{ id: string }>>(
            customerSearchResponse,
        )
        expect(candidatePage.items.map((candidate) => candidate.id)).toContain(
            customer.id,
        )
        await expect(customerInput).toHaveAttribute("aria-busy", "false")
        await chooseOption(salesPage, customerInput, customerName, customerName)
        await chooseOption(
            salesPage,
            dialog.locator("#sales-selection-create-owner"),
            fixture.sales.name,
        )
        await dialog.locator("#sales-selection-create-org").fill(fixture.orgId)
        await dialog
            .locator("#sales-selection-create-access-password")
            .fill(accessPassword)
        const created = await clickWrite<Book>(
            salesPage,
            "#sales-selection-create-submit",
            "/admin/sales-selection-books",
        )
        expect(created.body).toMatchObject({
            customer_id: customer.id,
            sales_owner_user_id: fixture.sales.id,
            business_org_unit_id: fixture.orgId,
            access_password: accessPassword,
        })
        expect(created.data.status).toBe("PREPARING")
        const replay = await write<Book>(
            "/admin/sales-selection-books",
            created.body,
            salesToken,
        )
        expect(replay.id).toBe(created.data.id)
        selectedBook = await prepared(salesToken, created.data.id)
        expect(selectedBook.items.map((row) => row.name).sort()).toEqual(
            fixture.skus.map((row) => row.name).sort(),
        )
        expect(selectedBook.display_count).toBe(3)
        const list = await apiGet<{ page: ApiPage<Book> }>(
            salesToken,
            "/admin/sales-selection-books",
            {
                customer_id: customer.id,
                page_size: 100,
            },
        )
        expect(list.page.items.map((row) => row.id)).toEqual([selectedBook.id])
    })

    await test.step("删除陈列只影响本册，发布必须确认当前准备批次", async () => {
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        const removed = selectedBook.items.find((row) => row.price === "45.00")!
        const [response] = await Promise.all([
            salesPage.waitForResponse(
                (candidate) =>
                    candidate.request().method() === "DELETE" &&
                    new URL(candidate.url()).pathname.endsWith(
                        `/display-items/${removed.item_id}`,
                    ),
            ),
            salesPage
                .locator(`[id^="sales-selection-preview-"][id$="-delete"]`)
                .filter({ hasText: "删除该项" })
                .nth(selectedBook.items.indexOf(removed))
                .click(),
        ])
        selectedBook = await uiResult<Book>(response)
        expect(selectedBook.display_count).toBe(2)
        expect(selectedBook.removed_count).toBe(1)
        expect(
            selectedBook.items.find((row) => row.item_id === removed.item_id)
                ?.removed,
        ).toBe(true)
        const rejected = await request<Book>(
            "POST",
            `/admin/sales-selection-books/${selectedBook.id}/publish`,
            {
                expected_version: selectedBook.version,
                idempotency_key: `missing-batch-${suffix}`,
            },
            salesToken,
        )
        expect(rejected.envelope.success).toBe(false)
        expect(rejected.envelope.errorMessage).toContain("准备批次")
        expect((await book(salesToken, selectedBook.id)).status).toBe(
            "PENDING_PUBLISH",
        )
        await dismissToasts(salesPage)
        const published = await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-publish",
            `/admin/sales-selection-books/${selectedBook.id}/publish`,
        )
        expect(published.body.batch_id).toBe(selectedBook.batch_id)
        expect(published.data.status).toBe("PUBLISHED")
        selectedBook = published.data
        const link = await apiGet<{ public_url: string }>(
            salesToken,
            `/admin/sales-selection-books/${selectedBook.id}/link`,
        )
        publicPath = link.public_url
        publicToken = tokenFromPath(publicPath)
        expect((await publicView(publicToken)).kind).toBe("LOCKED")
        const wrongPassword = await request(
            "POST",
            `/public/selection/${publicToken}/unlock`,
            {
                password: "incorrect-password",
            },
        )
        expect(
            wrongPassword.response.ok &&
                wrongPassword.envelope.success !== false,
        ).toBe(false)
        const unlocked = await unlock(publicToken, accessPassword)
        accessToken = unlocked.access_token
        const view = unlocked.page
        expect(view.kind).toBe("SELECTING")
        expect(view.items).toHaveLength(2)
        assertPublicFields(view)
        selected = view.items.find((row) => row.price === "25.00")!
        expect(
            selected.cover_path,
            "准备任务必须保存实际 S3 图片快照",
        ).toBeTruthy()
        await assertLocked(publicToken, selected)
        const image = await fetch(
            `${API_BASE}/public/selection/${publicToken}/images?ref=${encodeURIComponent(selected.cover_path!)}&access_token=${encodeURIComponent(accessToken)}`,
        )
        expect(image.status).toBe(200)
        expect(image.headers.get("cache-control")).toBe("private, no-store")
        expect(
            Buffer.from(await image.arrayBuffer()).byteLength,
        ).toBeGreaterThan(0)
        await expect(
            salesPage.locator(
                '[id^="sales-selection-preview-"][id$="-delete"]',
            ),
        ).toHaveCount(0)
    })

    await test.step("历史已发布册缺访问密码时关闭公开读取，销售设置密码后恢复原链接", async () => {
        const previousAccess = accessToken
        prepareLegacySelectionBook(selectedBook.id, customerName)
        await assertLocked(publicToken, selected, previousAccess)
        const legacyUnlock = await request(
            "POST",
            `/public/selection/${publicToken}/unlock`,
            {
                password: accessPassword,
            },
        )
        expect(legacyUnlock.response.status).toBe(403)
        expect(legacyUnlock.envelope.success).toBe(false)
        await salesPage.reload()
        await salesPage
            .locator("#sales-selection-detail-access-password")
            .click()
        await salesPage
            .locator("#sales-selection-access-password-input")
            .fill(accessPassword)
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-access-password-submit",
            `/admin/sales-selection-booklets/${selectedBook.id}/access-password`,
        )
        expect(
            (
                await apiGet<{ public_url: string }>(
                    salesToken,
                    `/admin/sales-selection-books/${selectedBook.id}/link`,
                )
            ).public_url,
        ).toBe(publicPath)
        accessToken = (await unlock(publicToken, accessPassword)).access_token
        expect((await publicView(publicToken, accessToken)).items).toEqual(
            selectedBook.items
                .filter((item) => !item.removed)
                .map((item) =>
                    expect.objectContaining({
                        item_id: item.item_id,
                        name: item.name,
                    }),
                ),
        )
    })

    await test.step("移动端保存选择由服务端计价，非法项与过期会话不得覆盖已存选择", async () => {
        await customerPage.goto(publicPath)
        await expect(
            customerPage.getByText(selected.name, { exact: true }),
        ).toHaveCount(0)
        await customerPage
            .locator("#sales-selection-public-password")
            .fill("incorrect-password")
        const [denied] = await Promise.all([
            customerPage.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    new URL(response.url()).pathname.endsWith("/unlock"),
            ),
            customerPage.locator("#sales-selection-public-unlock").click(),
        ])
        expect(((await denied.json()) as Envelope<unknown>).success).toBe(false)
        await expect(
            customerPage.locator("#sales-selection-public-password"),
        ).toBeVisible()
        accessToken = (
            await unlockViaUi(customerPage, publicToken, accessPassword)
        ).access_token
        await expect(
            customerPage.getByText(selected.name, { exact: true }).first(),
        ).toBeVisible({
            timeout: 20_000,
        })
        await customerPage
            .locator(
                `label[for="sales-selection-public-select-${selected.item_id}"]`,
            )
            .click()
        await customerPage
            .locator(`#sales-selection-public-qty-${selected.item_id}`)
            .fill("3")
        const saved = await clickWrite<PublicView>(
            customerPage,
            "#sales-selection-public-save",
            `/public/selection/${publicToken}/session`,
        )
        savedBody = saved.body
        expect(saved.data.total_amount).toBe("75.00")
        expect(saved.data.choices).toEqual([
            { item_id: selected.item_id, quantity: 3, line_amount: "75.00" },
        ])
        const stale = await request<PublicView>(
            "POST",
            `/public/selection/${publicToken}/session`,
            {
                expected_session_version: saved.data.session_version - 1,
                idempotency_key: `stale-session-${suffix}`,
                choices: [{ item_id: selected.item_id, quantity: 9 }],
            },
            undefined,
            accessToken,
        )
        expect(stale.response.status).toBe(409)
        const foreign = await request<PublicView>(
            "POST",
            `/public/selection/${publicToken}/session`,
            {
                expected_session_version: saved.data.session_version,
                idempotency_key: `foreign-choice-${suffix}`,
                choices: [{ item_id: "e2e-other-book-item", quantity: 1 }],
            },
            undefined,
            accessToken,
        )
        expect(foreign.envelope.success).toBe(false)
        for (const quantity of [0, 100001]) {
            const invalid = await request<PublicView>(
                "POST",
                `/public/selection/${publicToken}/session`,
                {
                    expected_session_version: saved.data.session_version,
                    idempotency_key: `quantity-${quantity}-${suffix}`,
                    choices: [{ item_id: selected.item_id, quantity }],
                },
                undefined,
                accessToken,
            )
            expect(
                invalid.envelope.success,
                "份数必须在 1 至 100000 之间",
            ).toBe(false)
        }
        const duplicate = await request<PublicView>(
            "POST",
            `/public/selection/${publicToken}/session`,
            {
                expected_session_version: saved.data.session_version,
                idempotency_key: `duplicate-choice-${suffix}`,
                choices: [
                    { item_id: selected.item_id, quantity: 1 },
                    { item_id: selected.item_id, quantity: 2 },
                ],
            },
            undefined,
            accessToken,
        )
        expect(duplicate.envelope.success, "同一陈列项不得重复保存").toBe(false)
        expect((await publicView(publicToken, accessToken)).choices).toEqual(
            saved.data.choices,
        )
        expect((await publicView(publicToken, accessToken)).total_amount).toBe(
            "75.00",
        )
        expect(
            (await publicView(publicToken, accessToken)).session_version,
        ).toBe(saved.data.session_version)
        await customerPage.reload()
        await expect(
            customerPage.locator(
                `#sales-selection-public-qty-${selected.item_id}`,
            ),
        ).toHaveValue("3")
    })

    await test.step("销售更换链接后旧令牌结束，新令牌保留会话与陈列", async () => {
        const previousToken = publicToken
        const previous = await publicView(previousToken, accessToken)
        const previousAccess = accessToken
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-replace-link",
            `/admin/sales-selection-books/${selectedBook.id}/replace-link`,
        )
        const link = await apiGet<{ public_url: string }>(
            salesToken,
            `/admin/sales-selection-books/${selectedBook.id}/link`,
        )
        publicPath = link.public_url
        publicToken = tokenFromPath(publicPath)
        expect(publicToken).not.toBe(previousToken)
        const oldLink = await request<PublicView>(
            "GET",
            `/public/selection/${previousToken}`,
        )
        expect(oldLink.response.status, "更换后旧令牌不得读取本册").toBe(404)
        expect(oldLink.envelope.success).toBe(false)
        await customerPage.reload()
        await expect(
            customerPage.getByRole("heading", {
                name: "选品链接已失效",
                exact: true,
            }),
        ).toBeVisible({ timeout: 20_000 })
        await assertLocked(publicToken, selected, previousAccess)
        const rotatedGrant = await unlock(publicToken, accessPassword)
        const rotated = rotatedGrant.page
        expect(rotated.items).toEqual(previous.items)
        expect(rotated.choices).toEqual(previous.choices)
        expect(rotated.session_version).toBe(previous.session_version)
        await customerPage.goto(publicPath)
        accessToken = (
            await unlockViaUi(customerPage, publicToken, accessPassword)
        ).access_token
        await expect(
            customerPage.locator(
                `#sales-selection-public-qty-${selected.item_id}`,
            ),
        ).toHaveValue("3")
    })

    await test.step("客户核对提交生成唯一冻结方案；重复提交与后续保存只读回执", async () => {
        await clickWrite<PublicView>(
            customerPage,
            "#sales-selection-public-review",
            `/public/selection/${publicToken}/session`,
        )
        const submitted = await clickWrite<PublicView>(
            customerPage,
            "#sales-selection-public-submit",
            `/public/selection/${publicToken}/submit`,
        )
        expect(submitted.data.kind).toBe("RECEIPT")
        expect(submitted.data.receipt?.total_amount).toBe("75.00")
        await expect(
            customerPage.getByRole("heading", {
                name: "选品已提交",
                exact: true,
            }),
        ).toBeVisible({ timeout: 20_000 })
        const replay = await write<PublicView>(
            `/public/selection/${publicToken}/submit`,
            submitted.body,
            undefined,
            "POST",
            accessToken,
        )
        expect(replay.receipt).toEqual(submitted.data.receipt)
        selectedBook = await book(salesToken, selectedBook.id)
        expect(selectedBook.status).toBe("SUBMITTED")
        const proposal = await apiGet<Proposal>(
            salesToken,
            `/admin/sales-selection-proposals/${selectedBook.proposal_id}`,
        )
        expect(proposal).toMatchObject({
            customer_id: customer.id,
            booklet_id: selectedBook.id,
            sales_owner_user_id: fixture.sales.id,
            business_org_unit_id: fixture.orgId,
            form: "SINGLE_SKU",
            submit_mode: "BY_QUANTITY",
            total_amount: "75.00",
        })
        expect(proposal.display_lines).toEqual([
            expect.objectContaining({
                display_item_id: selected.item_id,
                quantity: 3,
                unit_price: "25.00",
                line_amount: "75.00",
            }),
        ])
        expect(proposal.sku_lines).toEqual([
            expect.objectContaining({
                name: selected.name,
                quantity: 3,
                unit_price: "25.00",
                line_amount: "75.00",
            }),
        ])
        const proposals = await apiGet<{ page: ApiPage<ProposalListRow> }>(
            salesToken,
            "/admin/sales-selection-proposals",
            { booklet_id: selectedBook.id, page_size: 100 },
        )
        expect(proposals.page.items.map((row) => row.id)).toEqual([proposal.id])
        const orders = await apiGet<ApiPage<{ id: string }>>(
            salesToken,
            "/admin/sales-orders",
            {
                customer_id: customer.id,
                page_size: 100,
            },
        )
        expect(orders.items, "客户确认选品方案不得直接创建销售单").toEqual([])
        const receivables = await apiGet<ApiPage<{ id: string }>>(
            adminToken,
            "/admin/receivable-accounts",
            { customer_id: customer.id, page_size: 100 },
        )
        expect(receivables.items, "客户确认选品方案不得产生应收").toEqual([])
        const lateSave = await write<PublicView>(
            `/public/selection/${publicToken}/session`,
            {
                expected_session_version: 0,
                idempotency_key: `late-save-${suffix}`,
                choices: [{ item_id: selected.item_id, quantity: 99 }],
            },
            undefined,
            "POST",
            accessToken,
        )
        expect(lateSave.kind).toBe("RECEIPT")
        expect(lateSave.receipt).toEqual(submitted.data.receipt)
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        await salesPage.locator("#sales-selection-detail-open-proposal").click()
        await expect(
            salesPage.getByRole("heading", { name: customerName, exact: true }),
        ).toBeVisible({
            timeout: 20_000,
        })
        await expect(
            salesPage.getByText(proposal.proposal_no, { exact: true }),
        ).toBeVisible()
        await expect(
            salesPage.getByText(selected.name, { exact: true }),
        ).toBeVisible()
        await salesPage.goto(`/sales/selection/${selectedBook.id}`)
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-revoke",
            `/admin/sales-selection-books/${selectedBook.id}/revoke-link`,
        )
        expect((await publicView(publicToken)).kind).toBe("ENDED")
        for (const [action, body] of [
            ["session", savedBody],
            ["submit", submitted.body],
        ] as const) {
            const endedReplay = await write<PublicView>(
                `/public/selection/${publicToken}/${action}`,
                body,
                undefined,
                "POST",
                accessToken,
            )
            expect(endedReplay.kind).toBe("ENDED")
            expect(endedReplay.items).toEqual([])
            expect(endedReplay.choices).toEqual([])
            expect(endedReplay.receipt).toBeFalsy()
            expect(endedReplay.recipient).toBeFalsy()
        }
        expect(
            await apiGet<Proposal>(
                salesToken,
                `/admin/sales-selection-proposals/${proposal.id}`,
            ),
        ).toEqual(proposal)
        expect(await book(salesToken, selectedBook.id)).toMatchObject({
            status: "SUBMITTED",
            proposal_id: proposal.id,
            link_revoked: true,
        })
        const revokedImage = await fetch(
            `${API_BASE}/public/selection/${publicToken}/images?ref=${encodeURIComponent(selected.cover_path!)}&access_token=${encodeURIComponent(accessToken)}`,
        )
        expect(revokedImage.ok, "撤销链接后图片读取资格同时失效").toBe(false)
        await customerPage.reload()
        await expect(
            customerPage.getByRole("heading", {
                name: "选品已结束",
                exact: true,
            }),
        ).toBeVisible({ timeout: 20_000 })
    })

    await test.step("套餐单档重生成保留冻结商品池，商城兑换只确认款式且无数量金额", async () => {
        const createInput = {
            customer_id: customer.id,
            sales_owner_user_id: fixture.sales.id,
            business_org_unit_id: fixture.orgId,
            access_password: accessPassword,
            form: "PACKAGE",
            submit_mode: "MALL_REDEEM",
            pool_source_kind: "SELECTION",
            sku_ids: fixture.skus.map((row) => row.id),
            tiers: [
                {
                    name: "60 元档",
                    target_amount: "60.00",
                    tolerance: "0.00",
                    expected_count: 1,
                    sku_count: 2,
                },
            ],
            idempotency_key: `package-book-${suffix}`,
        }
        const created = await write<Book>(
            "/admin/sales-selection-books",
            createInput,
            salesToken,
        )
        let packageBook = await prepared(salesToken, created.id)
        expect(packageBook.display_count).toBe(1)
        expect(packageBook.items[0]).toMatchObject({
            price: "60.00",
            members: expect.arrayContaining([
                expect.objectContaining({ price: "25.00" }),
                expect.objectContaining({ price: "35.00" }),
            ]),
        })
        const frozen = {
            preparedAt: packageBook.prepared_at,
            eligibilityAsOf: packageBook.eligibility_as_of,
            batch: packageBook.batch_id,
            itemIds: packageBook.items.map((item) => item.item_id),
        }
        await salesPage.goto(`/sales/selection/${created.id}`)
        await clickWrite<Book>(
            salesPage,
            `#selection-regenerate-inline-${packageBook.tiers[0]!.tier_id}`,
            `/admin/sales-selection-books/${created.id}/prepare`,
        )
        packageBook = await prepared(salesToken, created.id)
        expect(packageBook.batch_id, "重生成沿用冻结批次").toBe(frozen.batch)
        expect(
            packageBook.items.map((item) => item.item_id),
            "重生成成功替换该档陈列",
        ).not.toEqual(frozen.itemIds)
        expect(packageBook.prepared_at).toBe(frozen.preparedAt)
        expect(packageBook.eligibility_as_of).toBe(frozen.eligibilityAsOf)
        await salesPage.reload()
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-publish",
            `/admin/sales-selection-books/${created.id}/publish`,
        )
        const link = await apiGet<{ public_url: string }>(
            salesToken,
            `/admin/sales-selection-books/${created.id}/link`,
        )
        const mallToken = tokenFromPath(link.public_url)
        await customerPage.goto(link.public_url)
        expect((await publicView(mallToken)).kind).toBe("LOCKED")
        const mallGrant = await unlockViaUi(
            customerPage,
            mallToken,
            accessPassword,
        )
        const mallView = mallGrant.page
        assertPublicFields(mallView)
        const mallItem = mallView.items[0]!
        await assertLocked(mallToken, mallItem)
        await customerPage
            .locator(
                `label[for="sales-selection-public-select-${mallItem.item_id}"]`,
            )
            .click()
        await expect(
            customerPage.locator('[id^="sales-selection-public-qty-"]'),
        ).toHaveCount(0)
        const confirmed = await clickWrite<PublicView>(
            customerPage,
            "#sales-selection-public-review",
            `/public/selection/${mallToken}/session`,
        )
        expect(confirmed.data.total_amount).toBeNull()
        expect(confirmed.data.choices).toEqual([
            { item_id: mallItem.item_id, quantity: null, line_amount: null },
        ])
        const withQuantity = await request<PublicView>(
            "POST",
            `/public/selection/${mallToken}/session`,
            {
                expected_session_version: confirmed.data.session_version,
                idempotency_key: `mall-quantity-${suffix}`,
                choices: [{ item_id: mallItem.item_id, quantity: 1 }],
            },
            undefined,
            mallGrant.access_token,
        )
        expect(
            withQuantity.envelope.success,
            "商城兑换只确认款式，不接受采购数量",
        ).toBe(false)
        expect(
            (await publicView(mallToken, mallGrant.access_token))
                .session_version,
        ).toBe(confirmed.data.session_version)
        const submitted = await clickWrite<PublicView>(
            customerPage,
            "#sales-selection-public-submit",
            `/public/selection/${mallToken}/submit`,
        )
        expect(submitted.data.receipt?.total_amount).toBeNull()
        const submittedBook = await book(salesToken, created.id)
        const proposal = await apiGet<Proposal>(
            salesToken,
            `/admin/sales-selection-proposals/${submittedBook.proposal_id}`,
        )
        expect(proposal).toMatchObject({
            form: "PACKAGE",
            submit_mode: "MALL_REDEEM",
            total_amount: null,
        })
        expect(proposal.sku_lines).toHaveLength(2)
        for (const line of [...proposal.sku_lines, ...proposal.display_lines]) {
            expect(line.quantity).toBeNull()
            expect(line.line_amount).toBeNull()
        }
    })

    await test.step("未提交册可关闭，待发布册可作废；终止后不得进入客户选品", async () => {
        const base = {
            customer_id: customer.id,
            sales_owner_user_id: fixture.sales.id,
            business_org_unit_id: fixture.orgId,
            access_password: accessPassword,
            form: "SINGLE_SKU",
            submit_mode: "BY_QUANTITY",
            pool_source_kind: "SELECTION",
            sku_ids: [fixture.skus[0]!.id],
        }
        const created = await write<Book>(
            "/admin/sales-selection-books",
            { ...base, idempotency_key: `close-book-${suffix}` },
            salesToken,
        )
        await prepared(salesToken, created.id)
        await salesPage.goto(`/sales/selection/${created.id}`)
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-publish",
            `/admin/sales-selection-books/${created.id}/publish`,
        )
        const link = await apiGet<{ public_url: string }>(
            salesToken,
            `/admin/sales-selection-books/${created.id}/link`,
        )
        const closed = await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-close",
            `/admin/sales-selection-books/${created.id}/close`,
        )
        expect(closed.data.status).toBe("CLOSED")
        expect((await publicView(tokenFromPath(link.public_url))).kind).toBe(
            "ENDED",
        )
        const draft = await write<Book>(
            "/admin/sales-selection-books",
            { ...base, idempotency_key: `void-book-${suffix}` },
            salesToken,
        )
        await prepared(salesToken, draft.id)
        await salesPage.goto(`/sales/selection/${draft.id}`)
        const voided = await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-void",
            `/admin/sales-selection-books/${draft.id}/void`,
        )
        expect(voided.data.status).toBe("VOIDED")
        const noLink = await request(
            "GET",
            `/admin/sales-selection-books/${draft.id}/link`,
            undefined,
            salesToken,
        )
        expect(noLink.envelope.success).toBe(false)
    })
    await publicContext.close()
})

test("[flow-27] 提货券按个人额度独立选品、冻结收件地址并导出全部明细", async ({
    browser,
}) => {
    test.setTimeout(360_000)
    const suffix = `${Date.now()}`
    const prefix = `E2E-VOUCHER-${suffix}`
    const originalPassword = `Voucher-${suffix}`
    const replacementPassword = `Voucher-new-${suffix}`
    const adminToken = await apiToken("admin")
    const fixture = await setupCatalog(adminToken, prefix)
    const salesToken = await apiToken("xiaoshou")
    const { page: salesPage } = await openLoggedInWorkspace(browser, "xiaoshou")
    const customerName = `E2E 提货券客户 ${suffix}`
    await createCustomerViaUi(salesPage, {
        legalName: customerName,
        shortName: `提货${suffix}`,
        paymentTermLabel: "货到 15 天",
    })
    const customerRows = await apiGet<
        ApiPage<{ id: string; legal_name: string }>
    >(salesToken, "/admin/customers", { keyword: customerName, page_size: 100 })
    const customer = customerRows.items.find(
        (row) => row.legal_name === customerName,
    )!
    expect(customer).toBeTruthy()
    let voucherBook!: Book
    let publicPath = ""
    let publicToken = ""
    let vouchers: Voucher[] = []
    let itemA!: Item
    let itemB!: Item
    const contexts = await Promise.all(
        Array.from({ length: 4 }, () =>
            browser.newContext({ viewport: { width: 390, height: 844 } }),
        ),
    )
    const pages = await Promise.all(
        contexts.map((context) => context.newPage()),
    )
    const grants: string[] = []
    const recipients: Recipient[] = [
        {
            name: "提货人甲",
            phone: "13800002701",
            province: "浙江省",
            city: "杭州市",
            district: "西湖区",
            address: "甲号路 27 号 101 室",
        },
        {
            name: "提货人乙",
            phone: "13800002702",
            province: "江苏省",
            city: "南京市",
            district: "鼓楼区",
            address: "乙号路 27 号 202 室",
        },
        {
            name: "提货人丙",
            phone: "13800002703",
            province: "上海市",
            city: "上海市",
            district: "浦东新区",
            address: "丙号路 27 号 303 室",
        },
        {
            name: "提货人丁",
            phone: "13800002704",
            province: "广东省",
            city: "深圳市",
            district: "南山区",
            address: "丁号路 27 号 404 室",
        },
    ]
    const receipts: PublicView[] = []
    const issuedByRecipient = new Map<string, Voucher>()
    let savedBodyA: Record<string, unknown> = {}
    let submittedBodyA: Record<string, unknown> = {}

    await test.step("真实页面发起提货券并为四人发行独立提货码，每人额度 60 元", async () => {
        await salesPage.goto(
            `/master-data/sellable-items?q=${encodeURIComponent(prefix)}`,
        )
        await expect(
            salesPage.getByText(fixture.skus[0]!.name, { exact: true }).first(),
        ).toBeVisible({
            timeout: 20_000,
        })
        await salesPage
            .locator("#master-data-sellable-items-launch-selection")
            .click()
        const dialog = salesPage.getByRole("dialog", {
            name: "发起选品",
            exact: true,
        })
        const customerInput = dialog.locator("#sales-selection-create-customer")
        await customerInput.click()
        const [search] = await Promise.all([
            salesPage.waitForResponse((response) => {
                const url = new URL(response.url())
                return (
                    response.request().method() === "GET" &&
                    url.pathname === "/admin/customers" &&
                    url.searchParams.get("keyword") === customerName
                )
            }),
            customerInput.fill(customerName),
        ])
        expect(
            (await uiResult<ApiPage<{ id: string }>>(search)).items.map(
                (row) => row.id,
            ),
        ).toContain(customer.id)
        await expect(customerInput).toHaveAttribute("aria-busy", "false")
        await chooseOption(salesPage, customerInput, customerName, customerName)
        await chooseOption(
            salesPage,
            dialog.locator("#sales-selection-create-owner"),
            fixture.sales.name,
        )
        await dialog.locator("#sales-selection-create-org").fill(fixture.orgId)
        await chooseOption(
            salesPage,
            dialog.locator("#sales-selection-create-mode"),
            "提货券",
        )
        await dialog
            .locator("#sales-selection-create-access-password")
            .fill(originalPassword)
        await dialog
            .locator("#sales-selection-create-per-person-budget")
            .fill("60.00")
        await dialog.locator("#sales-selection-create-voucher-count").fill("4")
        const created = await clickWrite<Book>(
            salesPage,
            "#sales-selection-create-submit",
            "/admin/sales-selection-books",
        )
        expect(created.body).toMatchObject({
            submit_mode: "PICKUP_VOUCHER",
            access_password: originalPassword,
            per_person_budget: "60.00",
            voucher_count: 4,
        })
        voucherBook = await prepared(salesToken, created.data.id)
        expect(voucherBook.per_person_budget).toBe("60.00")
        await salesPage.goto(`/sales/selection/${voucherBook.id}`)
        await expect(
            salesPage.locator("#sales-selection-detail-export-vouchers"),
        ).toBeDisabled()
        await expect(
            salesPage.getByText("发布后生成提货码", { exact: true }),
        ).toBeVisible()
        expect(
            await apiGet<Voucher[]>(
                salesToken,
                `/admin/sales-selection-books/${voucherBook.id}/vouchers`,
            ),
        ).toEqual([])
        voucherBook = (
            await clickWrite<Book>(
                salesPage,
                "#sales-selection-detail-publish",
                `/admin/sales-selection-books/${voucherBook.id}/publish`,
            )
        ).data
        vouchers = await apiGet<Voucher[]>(
            salesToken,
            `/admin/sales-selection-books/${voucherBook.id}/vouchers`,
        )
        expect(vouchers).toHaveLength(4)
        expect(
            new Set(vouchers.map((voucher) => voucher.voucher_code)).size,
        ).toBe(4)
        expect(
            vouchers.every(
                (voucher) => !voucher.submitted && !voucher.proposal_id,
            ),
        ).toBe(true)
        publicPath = (
            await apiGet<{ public_url: string }>(
                salesToken,
                `/admin/sales-selection-books/${voucherBook.id}/link`,
            )
        ).public_url
        publicToken = tokenFromPath(publicPath)
        const noCode = await request(
            "POST",
            `/public/selection/${publicToken}/unlock`,
            {
                password: originalPassword,
            },
        )
        expect(
            noCode.response.ok && noCode.envelope.success !== false,
            "提货券不能只凭册密码领取个人额度",
        ).toBe(false)
        const invalidCode = await request(
            "POST",
            `/public/selection/${publicToken}/unlock`,
            {
                password: originalPassword,
                voucher_code: "unknown-voucher",
            },
        )
        expect(
            invalidCode.response.ok && invalidCode.envelope.success !== false,
        ).toBe(false)
        for (const [index, page] of pages.entries()) {
            issuedByRecipient.set(recipients[index]!.name, vouchers[index]!)
            await page.goto(publicPath)
            if (index === 0) {
                await expect(
                    page.locator("#sales-selection-public-password"),
                ).toBeVisible()
                await assertMobileWidth(page)
            }
            const unlocked = await unlockViaUi(
                page,
                publicToken,
                originalPassword,
                vouchers[index]!.voucher_code,
            )
            grants[index] = unlocked.access_token
            expect(unlocked.page.participant_id).toBe(
                vouchers[index]!.participant_id,
            )
            expect(unlocked.page.kind).toBe("SELECTING")
            expect(unlocked.page.choices).toEqual([])
            expect(unlocked.page.receipt).toBeFalsy()
            expect(unlocked.page.recipient).toBeNull()
            expect(unlocked.page.per_person_budget).toBe("60.00")
            await expect(
                page.locator("#sales-selection-public-budget-remaining"),
            ).toHaveText("剩余额度 ¥60.00")
            itemA = unlocked.page.items.find((item) => item.price === "25.00")!
            itemB = unlocked.page.items.find((item) => item.price === "35.00")!
            if (index === 0) {
                await expect(
                    page.locator("#sales-selection-public-save"),
                ).toBeVisible()
                await assertMobileWidth(page)
            }
        }
        expect(itemA && itemB && itemA.cover_path).toBeTruthy()
        await assertLocked(publicToken, itemA)
    })

    await test.step("甲保存 50 元选品，服务端拒绝超额且不覆盖已存结果，乙看不到甲的选品", async () => {
        const pageA = pages[0]!
        await pageA
            .locator(
                `label[for="sales-selection-public-select-${itemA.item_id}"]`,
            )
            .click()
        await pageA
            .locator(`#sales-selection-public-qty-${itemA.item_id}`)
            .fill("2")
        await expect(
            pageA.locator("#sales-selection-public-budget-remaining"),
        ).toHaveText("剩余额度 ¥10.00")
        const saved = await clickWrite<PublicView>(
            pageA,
            "#sales-selection-public-save",
            `/public/selection/${publicToken}/session`,
        )
        savedBodyA = saved.body
        expect(saved.data.total_amount).toBe("50.00")
        expect(saved.data.choices).toEqual([
            { item_id: itemA.item_id, quantity: 2, line_amount: "50.00" },
        ])
        await pageA
            .locator(`#sales-selection-public-qty-${itemA.item_id}`)
            .fill("1")
        await pageA
            .locator(
                `label[for="sales-selection-public-select-${itemB.item_id}"]`,
            )
            .click()
        await expect(
            pageA.locator("#sales-selection-public-budget-remaining"),
        ).toHaveText("剩余额度 ¥0.00")
        await expect(
            pageA.locator("#sales-selection-public-save"),
        ).toBeEnabled()
        await expect(
            pageA.locator("#sales-selection-public-review"),
        ).toBeEnabled()
        await pageA
            .locator(
                `label[for="sales-selection-public-select-${itemB.item_id}"]`,
            )
            .click()
        await pageA
            .locator(`#sales-selection-public-qty-${itemA.item_id}`)
            .fill("3")
        await expect(
            pageA.locator("#sales-selection-public-budget-remaining"),
        ).toHaveText("剩余额度 ¥-15.00")
        await expect(
            pageA.locator("#sales-selection-public-budget"),
        ).toContainText("已超额")
        await expect(
            pageA.locator("#sales-selection-public-save"),
        ).toBeDisabled()
        await expect(
            pageA.locator("#sales-selection-public-review"),
        ).toBeDisabled()
        await pageA
            .locator(`#sales-selection-public-qty-${itemA.item_id}`)
            .fill("2")
        await expect(
            pageA.locator("#sales-selection-public-budget-remaining"),
        ).toHaveText("剩余额度 ¥10.00")
        await expect(
            pageA.locator("#sales-selection-public-save"),
        ).toBeEnabled()
        await expect(
            pageA.locator("#sales-selection-public-review"),
        ).toBeEnabled()
        const exceeded = await request<PublicView>(
            "POST",
            `/public/selection/${publicToken}/session`,
            {
                expected_session_version: saved.data.session_version,
                idempotency_key: `voucher-over-budget-${suffix}`,
                choices: [{ item_id: itemA.item_id, quantity: 3 }],
            },
            undefined,
            grants[0],
        )
        expect(
            exceeded.response.ok && exceeded.envelope.success !== false,
            "每人超额必须由服务端拒绝",
        ).toBe(false)
        const unchanged = await publicView(publicToken, grants[0])
        expect(unchanged.choices).toEqual(saved.data.choices)
        expect(unchanged.total_amount).toBe("50.00")
        expect(unchanged.session_version).toBe(saved.data.session_version)
        expect((await publicView(publicToken, grants[1])).choices).toEqual([])
        await pages[1]!.reload()
        await expect(
            pages[1]!.locator(
                `#sales-selection-public-select-${itemA.item_id}`,
            ),
        ).not.toBeChecked()
        await pageA.reload()
        await expect(
            pageA.locator(`#sales-selection-public-qty-${itemA.item_id}`),
        ).toHaveValue("2")
        await expect(
            pageA.locator("#sales-selection-public-password"),
        ).toHaveCount(0)
    })

    await test.step("地址缺失或不完整不得生成方案；甲提交只冻结个人，乙仍可继续选品", async () => {
        const pageA = pages[0]!
        const reviewed = await clickWrite<PublicView>(
            pageA,
            "#sales-selection-public-review",
            `/public/selection/${publicToken}/session`,
        )
        for (const recipient of [
            undefined,
            { ...recipients[0]!, address: "" },
        ]) {
            const missingAddress = await request<PublicView>(
                "POST",
                `/public/selection/${publicToken}/submit`,
                {
                    expected_session_version: reviewed.data.session_version,
                    idempotency_key: `voucher-missing-address-${recipient ? "partial" : "none"}-${suffix}`,
                    ...(recipient ? { recipient } : {}),
                },
                undefined,
                grants[0],
            )
            expect(
                missingAddress.response.ok &&
                    missingAddress.envelope.success !== false,
                "个人收件地址必须完整",
            ).toBe(false)
        }
        expect(
            (
                await apiGet<{ page: ApiPage<ProposalListRow> }>(
                    salesToken,
                    "/admin/sales-selection-proposals",
                    {
                        booklet_id: voucherBook.id,
                        page_size: 100,
                    },
                )
            ).page.items,
        ).toEqual([])
        expect((await publicView(publicToken, grants[0])).session_version).toBe(
            reviewed.data.session_version,
        )
        await fillRecipient(pageA, recipients[0]!)
        await assertMobileWidth(pageA)
        const submitRoute = `**/public/selection/${publicToken}/submit`
        let interruptedBody: Record<string, unknown> | undefined
        await pageA.route(submitRoute, async (route) => {
            interruptedBody = route.request().postDataJSON() as Record<
                string,
                unknown
            >
            await route.abort("failed")
        })
        await pageA.locator("#sales-selection-public-submit").click()
        await expect(
            pageA.locator("#sales-selection-public-reconcile"),
        ).toBeVisible()
        expect(interruptedBody?.recipient).toEqual(recipients[0])
        for (const [key, value] of Object.entries(recipients[0]!))
            await expect(
                pageA.locator(`#sales-selection-public-recipient-${key}`),
            ).toHaveValue(value)
        await pageA.unroute(submitRoute)
        await pageA.reload()
        await expect(
            pageA.locator("#sales-selection-public-reconcile"),
        ).toBeVisible()
        for (const [key, value] of Object.entries(recipients[0]!))
            await expect(
                pageA.locator(`#sales-selection-public-recipient-${key}`),
            ).toHaveValue(value)
        await assertMobileWidth(pageA)
        const submitted = await clickWrite<PublicView>(
            pageA,
            "#sales-selection-public-reconcile",
            `/public/selection/${publicToken}/submit`,
        )
        expect(
            submitted.body,
            "网络结果未知时恢复必须重用原收件信息和幂等命令",
        ).toEqual(interruptedBody)
        expect(submitted.body.recipient).toEqual(recipients[0])
        submittedBodyA = submitted.body
        receipts[0] = submitted.data
        expect(submitted.data.kind).toBe("RECEIPT")
        expect(submitted.data.recipient).toEqual(recipients[0])
        expect(submitted.data.receipt?.total_amount).toBe("50.00")
        const replay = await write<PublicView>(
            `/public/selection/${publicToken}/submit`,
            submitted.body,
            undefined,
            "POST",
            grants[0],
        )
        expect(replay.receipt).toEqual(submitted.data.receipt)
        const changedRecipientReplay = await request<PublicView>(
            "POST",
            `/public/selection/${publicToken}/submit`,
            { ...submitted.body, recipient: recipients[1] },
            undefined,
            grants[0],
        )
        expect(
            changedRecipientReplay.response.status,
            "同一提交命令不能重放为另一收件人",
        ).toBe(409)
        const savedCommandReplay = await write<PublicView>(
            `/public/selection/${publicToken}/session`,
            savedBodyA,
            undefined,
            "POST",
            grants[0],
        )
        expect(savedCommandReplay.kind).toBe("RECEIPT")
        expect(savedCommandReplay.receipt).toEqual(submitted.data.receipt)
        const laterSave = await write<PublicView>(
            `/public/selection/${publicToken}/session`,
            {
                expected_session_version: 0,
                idempotency_key: `voucher-late-save-${suffix}`,
                choices: [{ item_id: itemB.item_id, quantity: 1 }],
            },
            undefined,
            "POST",
            grants[0],
        )
        expect(laterSave.receipt).toEqual(submitted.data.receipt)
        expect(laterSave.recipient).toEqual(recipients[0])
        await pageA.reload()
        await expect(
            pageA.getByRole("heading", { name: "选品已提交", exact: true }),
        ).toBeVisible({ timeout: 20_000 })
        await expect(pageA.getByLabel("收件信息")).toContainText(
            recipients[0]!.phone,
        )
        await expect(pageA.getByLabel("收件信息")).toContainText(
            recipients[0]!.address,
        )
        await assertMobileWidth(pageA)
        expect((await publicView(publicToken, grants[0])).recipient).toEqual(
            recipients[0],
        )
        const restoredA = await unlock(
            publicToken,
            originalPassword,
            issuedByRecipient.get(recipients[0]!.name)!.voucher_code,
        )
        expect(restoredA.page.recipient).toEqual(recipients[0])
        expect(restoredA.page.receipt).toEqual(submitted.data.receipt)
        expect((await book(salesToken, voucherBook.id)).status).toBe(
            "PUBLISHED",
        )
        const independentB = await publicView(publicToken, grants[1])
        expect(independentB.kind).toBe("SELECTING")
        expect(independentB.receipt).toBeFalsy()
        expect(independentB.recipient).toBeNull()
        const pageB = pages[1]!
        await pageB
            .locator(
                `label[for="sales-selection-public-select-${itemB.item_id}"]`,
            )
            .click()
        const savedB = await clickWrite<PublicView>(
            pageB,
            "#sales-selection-public-save",
            `/public/selection/${publicToken}/session`,
        )
        expect(savedB.data.total_amount).toBe("35.00")
        expect(savedB.data.choices).toEqual([
            { item_id: itemB.item_id, quantity: 1, line_amount: "35.00" },
        ])
        expect((await publicView(publicToken, grants[0])).receipt).toEqual(
            submitted.data.receipt,
        )
    })

    await test.step("销售更换访问密码后旧授权失效，新密码保留个人回执和未提交选择", async () => {
        await salesPage.goto(`/sales/selection/${voucherBook.id}`)
        await salesPage
            .locator("#sales-selection-detail-access-password")
            .click()
        await salesPage
            .locator("#sales-selection-access-password-input")
            .fill(replacementPassword)
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-access-password-submit",
            `/admin/sales-selection-booklets/${voucherBook.id}/access-password`,
        )
        for (const oldGrant of grants)
            await assertLocked(publicToken, itemA, oldGrant)
        const oldSubmitReplay = await request<PublicView>(
            "POST",
            `/public/selection/${publicToken}/submit`,
            submittedBodyA,
            undefined,
            grants[0],
        )
        expect(
            oldSubmitReplay.response.status,
            "换密码后旧授权不得通过幂等回放读取个人地址",
        ).toBe(403)
        const oldPassword = await request(
            "POST",
            `/public/selection/${publicToken}/unlock`,
            {
                password: originalPassword,
                voucher_code: vouchers[0]!.voucher_code,
            },
        )
        expect(
            oldPassword.response.ok && oldPassword.envelope.success !== false,
        ).toBe(false)
        for (const [index, page] of pages.entries()) {
            await page.reload()
            const unlocked = await unlockViaUi(
                page,
                publicToken,
                replacementPassword,
                vouchers[index]!.voucher_code,
            )
            grants[index] = unlocked.access_token
            if (index === 0) {
                expect(unlocked.page.receipt).toEqual(receipts[0]!.receipt)
                expect(unlocked.page.recipient).toEqual(recipients[0])
            } else if (index === 1) {
                expect(unlocked.page.choices).toEqual([
                    {
                        item_id: itemB.item_id,
                        quantity: 1,
                        line_amount: "35.00",
                    },
                ])
                expect(unlocked.page.receipt).toBeFalsy()
                expect(unlocked.page.recipient).toBeNull()
            } else {
                expect(unlocked.page.choices).toEqual([])
                expect(unlocked.page.receipt).toBeFalsy()
                expect(unlocked.page.recipient).toBeNull()
            }
        }
        const pageB = pages[1]!
        await clickWrite<PublicView>(
            pageB,
            "#sales-selection-public-review",
            `/public/selection/${publicToken}/session`,
        )
        await fillRecipient(pageB, recipients[1]!)
        receipts[1] = (
            await clickWrite<PublicView>(
                pageB,
                "#sales-selection-public-submit",
                `/public/selection/${publicToken}/submit`,
            )
        ).data
        expect(receipts[1]!.receipt?.total_amount).toBe("35.00")
        expect(receipts[1]!.recipient).toEqual(recipients[1])
        expect((await book(salesToken, voucherBook.id)).status).toBe(
            "PUBLISHED",
        )
    })

    await test.step("两个独立浏览器同时提交仍分别生成一份个人方案与地址快照", async () => {
        for (const index of [2, 3]) {
            const page = pages[index]!
            const item = index === 2 ? itemA : itemB
            await page
                .locator(
                    `label[for="sales-selection-public-select-${item.item_id}"]`,
                )
                .click()
            await clickWrite<PublicView>(
                page,
                "#sales-selection-public-review",
                `/public/selection/${publicToken}/session`,
            )
            await fillRecipient(page, recipients[index]!)
        }
        const concurrent = await Promise.all(
            [2, 3].map((index) =>
                clickWrite<PublicView>(
                    pages[index]!,
                    "#sales-selection-public-submit",
                    `/public/selection/${publicToken}/submit`,
                ),
            ),
        )
        concurrent.forEach((submitted, offset) => {
            const index = offset + 2
            receipts[index] = submitted.data
            expect(submitted.data.kind).toBe("RECEIPT")
            expect(submitted.data.recipient).toEqual(recipients[index])
            expect(submitted.body.recipient).toEqual(recipients[index])
            expect(submitted.data.receipt?.total_amount).toBe(
                index === 2 ? "25.00" : "35.00",
            )
        })
        const proposals = await apiGet<{ page: ApiPage<ProposalListRow> }>(
            salesToken,
            "/admin/sales-selection-proposals",
            { booklet_id: voucherBook.id, page_size: 100 },
        )
        expect(proposals.page.items).toHaveLength(4)
        expect(
            new Set(proposals.page.items.map((proposal) => proposal.id)).size,
        ).toBe(4)
        const listJson = JSON.stringify(proposals.page.items)
        for (const proposal of proposals.page.items)
            expect(proposal, "方案列表不得返回个人收件信息").not.toHaveProperty(
                "recipient",
            )
        for (const recipient of recipients)
            for (const value of Object.values(recipient))
                expect(
                    listJson,
                    "方案列表不得泄露姓名、电话或地址",
                ).not.toContain(value)
        const expectedSelections = [
            { item: itemA, quantity: 2, total: "50.00" },
            { item: itemB, quantity: 1, total: "35.00" },
            { item: itemA, quantity: 1, total: "25.00" },
            { item: itemB, quantity: 1, total: "35.00" },
        ]
        for (const [index, recipient] of recipients.entries()) {
            const listRow = proposals.page.items.find(
                (proposal) =>
                    proposal.participant_id === vouchers[index]!.participant_id,
            )!
            expect(listRow, `个人方案缺失：${recipient.name}`).toBeTruthy()
            const proposal = await apiGet<Proposal>(
                salesToken,
                `/admin/sales-selection-proposals/${listRow.id}`,
            )
            const expected = expectedSelections[index]!
            expect(proposal).toMatchObject({
                booklet_id: voucherBook.id,
                customer_id: customer.id,
                sales_owner_user_id: fixture.sales.id,
                business_org_unit_id: fixture.orgId,
                submit_mode: "PICKUP_VOUCHER",
                recipient,
                total_amount: expected.total,
            })
            expect(proposal.sku_lines).toEqual([
                expect.objectContaining({
                    name: expected.item.name,
                    quantity: expected.quantity,
                    unit_price: expected.item.price,
                    line_amount: expected.total,
                }),
            ])
            expect(proposal.proposal_no).toBe(
                receipts[index]!.receipt?.proposal_no,
            )
            await pages[index]!.reload()
            await expect(
                pages[index]!.getByRole("heading", {
                    name: "选品已提交",
                    exact: true,
                }),
            ).toBeVisible({ timeout: 20_000 })
            const restored = await unlock(
                publicToken,
                replacementPassword,
                vouchers[index]!.voucher_code,
            )
            expect(restored.page.receipt).toEqual(receipts[index]!.receipt)
            expect(restored.page.recipient).toEqual(recipient)
            expect(
                (await publicView(publicToken, grants[index])).recipient,
            ).toEqual(recipient)
        }
        expect((await book(salesToken, voucherBook.id)).status).toBe(
            "PUBLISHED",
        )
        const latestVouchers = await apiGet<Voucher[]>(
            salesToken,
            `/admin/sales-selection-books/${voucherBook.id}/vouchers`,
        )
        expect(
            latestVouchers.every(
                (voucher) => voucher.submitted && voucher.proposal_id,
            ),
        ).toBe(true)
        expect(
            new Set(latestVouchers.map((voucher) => voucher.proposal_id)).size,
        ).toBe(4)
        expect(latestVouchers).toHaveLength(vouchers.length)
        for (const issued of issuedByRecipient.values()) {
            const current = latestVouchers.find(
                (candidate) => candidate.voucher_code === issued.voucher_code,
            )!
            expect(current.participant_id).toBe(issued.participant_id)
            expect(current.proposal_id).toBe(
                proposals.page.items.find(
                    (proposal) =>
                        proposal.participant_id === issued.participant_id,
                )?.id,
            )
        }
        expect(
            (
                await apiGet<ApiPage<{ id: string }>>(
                    salesToken,
                    "/admin/sales-orders",
                    {
                        customer_id: customer.id,
                        page_size: 100,
                    },
                )
            ).items,
        ).toEqual([])
    })

    await test.step("后台汇总所有个人商品与地址，真实下载提货码及完整选品 XLSX", async () => {
        const details = await apiGet<SelectionDetail[]>(
            salesToken,
            `/admin/sales-selection-books/${voucherBook.id}/selection-details`,
        )
        expect(details).toHaveLength(4)
        for (const [index, recipient] of recipients.entries()) {
            const detail = details.find(
                (row) => row.recipient.name === recipient.name,
            )!
            expect(detail.recipient).toEqual(recipient)
            expect(detail.total_amount).toBe(
                index === 0 ? "50.00" : index === 2 ? "25.00" : "35.00",
            )
            expect(detail.voucher_code).toBe(
                issuedByRecipient.get(recipient.name)!.voucher_code,
            )
            expect(detail.participant_id).toBe(
                issuedByRecipient.get(recipient.name)!.participant_id,
            )
            expect(detail.proposal_no).toBe(
                receipts[index]!.receipt?.proposal_no,
            )
            expect(detail.items).toEqual([
                expect.objectContaining({
                    name: index % 2 === 0 ? itemA.name : itemB.name,
                    quantity: index === 0 ? 2 : 1,
                    line_amount:
                        index === 0 ? "50.00" : index === 2 ? "25.00" : "35.00",
                }),
            ])
        }
        await salesPage.goto(`/sales/selection/${voucherBook.id}`)
        for (const recipient of recipients)
            await expect(
                salesPage.getByText(recipient.name).first(),
            ).toBeVisible()
        for (const [kind, filename, sheetName] of [
            [
                "vouchers",
                `提货码-${customerName.replace(/[^\p{L}\p{N}_-]/gu, "_")}.xlsx`,
                "提货码",
            ],
            [
                "selection-details",
                `选品明细-${customerName.replace(/[^\p{L}\p{N}_-]/gu, "_")}.xlsx`,
                "选品明细",
            ],
        ]) {
            const [download] = await Promise.all([
                salesPage.waitForEvent("download"),
                salesPage
                    .locator(`#sales-selection-detail-export-${kind}`)
                    .click(),
            ])
            expect(download.suggestedFilename()).toBe(filename)
            expect(await download.failure()).toBeNull()
            const downloadPath = await download.path()
            expect(downloadPath).toBeTruthy()
            const workbook = await readImportWorkbook(downloadPath!)
            const sheet = workbook.getWorksheet(sheetName!)!
            expect(sheet, "导出必须包含约定工作表").toBeTruthy()
            expect(sheet.rowCount).toBe(5)
            const rows = sheet.getRows(2, 4)!
            if (kind === "vouchers") {
                expect(rows.map((row) => row.getCell(1).text).sort()).toEqual(
                    vouchers.map((voucher) => voucher.voucher_code).sort(),
                )
                expect(
                    rows.every((row) => row.getCell(2).text.includes("已提交")),
                ).toBe(true)
            } else {
                expect(sheet.getRow(1).getCell(4).text).toBe("收件人")
                expect(sheet.getRow(1).getCell(10).text).toBe("商品名称")
                for (const [index, recipient] of recipients.entries()) {
                    const row = rows.find(
                        (candidate) =>
                            candidate.getCell(4).text === recipient.name,
                    )!
                    expect(
                        row,
                        `导出个人明细缺失：${recipient.name}`,
                    ).toBeTruthy()
                    expect(
                        [5, 6, 7, 8, 9].map(
                            (column) => row.getCell(column).text,
                        ),
                    ).toEqual([
                        recipient.phone,
                        recipient.province,
                        recipient.city,
                        recipient.district,
                        recipient.address,
                    ])
                    expect(row.getCell(1).text).toBe(
                        issuedByRecipient.get(recipient.name)!.voucher_code,
                    )
                    expect(row.getCell(2).text).toBe(
                        receipts[index]!.receipt?.proposal_no,
                    )
                    expect(row.getCell(10).text).toBe(
                        index % 2 === 0 ? itemA.name : itemB.name,
                    )
                    expect(row.getCell(12).text).toBe(index === 0 ? "2" : "1")
                    for (const column of [5, 12, 14, 15, 16])
                        expect(typeof row.getCell(column).value).toBe("string")
                    expect(row.getCell(15).text).toBe(
                        index === 0 ? "50.00" : index === 2 ? "25.00" : "35.00",
                    )
                    expect(row.getCell(16).text).toBe(row.getCell(15).text)
                }
            }
        }
    })
    await test.step("关闭整册后个人旧命令回放不能读取商品、回执或收件信息", async () => {
        await clickWrite<Book>(
            salesPage,
            "#sales-selection-detail-close",
            `/admin/sales-selection-books/${voucherBook.id}/close`,
        )
        for (const [action, body] of [
            ["session", savedBodyA],
            ["submit", submittedBodyA],
        ] as const) {
            const endedReplay = await write<PublicView>(
                `/public/selection/${publicToken}/${action}`,
                body,
                undefined,
                "POST",
                grants[0],
            )
            expect(endedReplay.kind).toBe("ENDED")
            expect(endedReplay.items).toEqual([])
            expect(endedReplay.choices).toEqual([])
            expect(endedReplay.receipt).toBeFalsy()
            expect(endedReplay.recipient).toBeFalsy()
        }
        for (const page of pages) {
            await page.reload()
            await expect(
                page.getByRole("heading", { name: "选品已结束", exact: true }),
            ).toBeVisible({ timeout: 20_000 })
        }
        const frozen = await apiGet<SelectionDetail[]>(
            salesToken,
            `/admin/sales-selection-books/${voucherBook.id}/selection-details`,
        )
        expect(frozen).toHaveLength(4)
        expect(frozen.map((detail) => detail.recipient.name).sort()).toEqual(
            recipients.map((recipient) => recipient.name).sort(),
        )
    })
    await Promise.all(contexts.map((context) => context.close()))
})

test("[flow-27] 套餐提货券保留商品来源、同码并发唯一方案及真实提交后响应丢失恢复", async ({
    browser,
}) => {
    test.setTimeout(300_000)
    const suffix = `${Date.now()}`
    const prefix = `E2E-PACK-VOUCHER-${suffix}`
    const password = `Package-voucher-${suffix}`
    const fixture = await setupCatalog(await apiToken("admin"), prefix)
    const salesToken = await apiToken("xiaoshou")
    const { page: salesPage } = await openLoggedInWorkspace(browser, "xiaoshou")
    const customerName = `E2E 套餐提货客户 ${suffix}`
    await createCustomerViaUi(salesPage, {
        legalName: customerName,
        shortName: `套餐提货${suffix}`,
        paymentTermLabel: "货到 15 天",
    })
    const customers = await apiGet<ApiPage<{ id: string; legal_name: string }>>(
        salesToken,
        "/admin/customers",
        { keyword: customerName, page_size: 100 },
    )
    const customer = customers.items.find(
        (row) => row.legal_name === customerName,
    )!
    expect(customer).toBeTruthy()
    const created = await write<Book>(
        "/admin/sales-selection-books",
        {
            customer_id: customer.id,
            sales_owner_user_id: fixture.sales.id,
            business_org_unit_id: fixture.orgId,
            access_password: password,
            form: "PACKAGE",
            submit_mode: "PICKUP_VOUCHER",
            per_person_budget: "200.00",
            voucher_count: 2,
            pool_source_kind: "SELECTION",
            sku_ids: fixture.skus.map((sku) => sku.id),
            tiers: ["60.00", "70.00"].map((target_amount) => ({
                name: `${target_amount} 元档`,
                target_amount,
                tolerance: "0.00",
                expected_count: 1,
                sku_count: 2,
            })),
            idempotency_key: `package-voucher-${suffix}`,
        },
        salesToken,
    )
    const preparedBook = await prepared(salesToken, created.id)
    expect(preparedBook.items).toHaveLength(2)
    const firstPackage = preparedBook.items.find(
        (item) => item.price === "60.00",
    )!
    const secondPackage = preparedBook.items.find(
        (item) => item.price === "70.00",
    )!
    expect(firstPackage.members.map((member) => member.price).sort()).toEqual([
        "25.00",
        "35.00",
    ])
    expect(secondPackage.members.map((member) => member.price).sort()).toEqual([
        "25.00",
        "45.00",
    ])
    expect(
        firstPackage.members.find((member) => member.price === "25.00")!.sku_id,
    ).toBe(fixture.skus[0]!.id)
    expect(
        secondPackage.members.find((member) => member.price === "25.00")!
            .sku_id,
    ).toBe(fixture.skus[0]!.id)
    await salesPage.goto(`/sales/selection/${created.id}`)
    await clickWrite<Book>(
        salesPage,
        "#sales-selection-detail-publish",
        `/admin/sales-selection-books/${created.id}/publish`,
    )
    const vouchers = await apiGet<Voucher[]>(
        salesToken,
        `/admin/sales-selection-books/${created.id}/vouchers`,
    )
    expect(vouchers).toHaveLength(2)
    const publicPath = (
        await apiGet<{ public_url: string }>(
            salesToken,
            `/admin/sales-selection-books/${created.id}/link`,
        )
    ).public_url
    const publicToken = tokenFromPath(publicPath)
    const endpoint = `/public/selection/${publicToken}/submit`
    const contexts = await Promise.all(
        Array.from({ length: 3 }, () =>
            browser.newContext({ viewport: { width: 390, height: 844 } }),
        ),
    )
    const pages = await Promise.all(
        contexts.map((context) => context.newPage()),
    )
    const recipients: Recipient[] = [
        {
            name: "套餐提货人甲",
            phone: "13800002711",
            province: "浙江省",
            city: "杭州市",
            district: "西湖区",
            address: "套餐甲路 27 号 101 室",
        },
        {
            name: "套餐提货人乙",
            phone: "13800002712",
            province: "江苏省",
            city: "南京市",
            district: "鼓楼区",
            address: "套餐乙路 27 号 202 室",
        },
    ]
    const grants: string[] = []
    const receipts: PublicView[] = []
    try {
        await test.step("套餐整套份数按成员展开，超额不得改写本人保存结果", async () => {
            const page = pages[0]!
            await page.goto(publicPath)
            grants[0] = (
                await unlockViaUi(
                    page,
                    publicToken,
                    password,
                    vouchers[0]!.voucher_code,
                )
            ).access_token
            for (const [item, quantity] of [
                [firstPackage, "2"],
                [secondPackage, "1"],
            ] as const) {
                await page
                    .locator(
                        `label[for="sales-selection-public-select-${item.item_id}"]`,
                    )
                    .click()
                await page
                    .locator(`#sales-selection-public-qty-${item.item_id}`)
                    .fill(quantity)
            }
            await expect(
                page.locator("#sales-selection-public-budget-remaining"),
            ).toHaveText("剩余额度 ¥10.00")
            const saved = await clickWrite<PublicView>(
                page,
                "#sales-selection-public-review",
                `/public/selection/${publicToken}/session`,
            )
            expect(saved.data.total_amount).toBe("190.00")
            expect(saved.data.choices).toHaveLength(2)
            expect(saved.data.choices).toEqual(
                expect.arrayContaining([
                    {
                        item_id: firstPackage.item_id,
                        quantity: 2,
                        line_amount: "120.00",
                    },
                    {
                        item_id: secondPackage.item_id,
                        quantity: 1,
                        line_amount: "70.00",
                    },
                ]),
            )
            const exceeded = await request<PublicView>(
                "POST",
                `/public/selection/${publicToken}/session`,
                {
                    expected_session_version: saved.data.session_version,
                    idempotency_key: `package-over-budget-${suffix}`,
                    choices: [
                        { item_id: firstPackage.item_id, quantity: 3 },
                        { item_id: secondPackage.item_id, quantity: 1 },
                    ],
                },
                undefined,
                grants[0],
            )
            expect(
                exceeded.response.ok && exceeded.envelope.success !== false,
            ).toBe(false)
            const unchanged = await publicView(publicToken, grants[0])
            expect(unchanged.choices).toEqual(saved.data.choices)
            expect(unchanged.session_version).toBe(saved.data.session_version)
            expect(unchanged.total_amount).toBe("190.00")
        })

        await test.step("同一码两个客户端到达请求屏障后并发提交，只保留一个个人方案且原命令可重放", async () => {
            await pages[1]!.goto(publicPath)
            const secondClient = await unlockViaUi(
                pages[1]!,
                publicToken,
                password,
                vouchers[0]!.voucher_code,
            )
            grants[1] = secondClient.access_token
            expect(secondClient.page.participant_id).toBe(
                vouchers[0]!.participant_id,
            )
            expect(secondClient.page.total_amount).toBe("190.00")
            const commands = [0, 1].map((index) => ({
                expected_session_version: secondClient.page.session_version,
                idempotency_key: `same-voucher-submit-${index}-${suffix}`,
                recipient: recipients[0],
            }))
            let arrived = 0
            let release!: () => void
            const barrier = new Promise<void>((resolve) => {
                release = resolve
            })
            const matcher = `**${endpoint}`
            for (const page of pages.slice(0, 2))
                await page.route(matcher, async (route) => {
                    if (route.request().method() !== "POST")
                        return route.fallback()
                    arrived += 1
                    await barrier
                    // 页面级拦截绕过共享前端的转发 fixture，必须显式指向本次 shard。
                    await route.continue({ url: `${API_BASE}${endpoint}` })
                })
            const pending = pages.slice(0, 2).map((page, index) =>
                page.evaluate(
                    async ({ url, accessToken, body }) => {
                        const response = await fetch(url, {
                            method: "POST",
                            headers: {
                                "Content-Type": "application/json",
                                "X-Selection-Access": accessToken,
                            },
                            body: JSON.stringify(body),
                            signal: AbortSignal.timeout(20_000),
                        })
                        return {
                            ok: response.ok,
                            status: response.status,
                            envelope:
                                (await response.json()) as Envelope<PublicView>,
                        }
                    },
                    {
                        url: `${API_BASE}${endpoint}`,
                        accessToken: grants[index]!,
                        body: commands[index]!,
                    },
                ),
            )
            try {
                await expect.poll(() => arrived, { timeout: 10_000 }).toBe(2)
                release()
                const submitted = await Promise.all(pending)
                const successful = submitted.filter(
                    (result) => result.ok && result.envelope.success !== false,
                )
                expect(
                    successful.length,
                    "同码并发至少一条命令成功",
                ).toBeGreaterThanOrEqual(1)
                receipts[0] = successful[0]!.envelope.data
                for (const result of submitted) {
                    if (result.ok && result.envelope.success !== false) {
                        expect(result.envelope.data.kind).toBe("RECEIPT")
                        expect(result.envelope.data.recipient).toEqual(
                            recipients[0],
                        )
                        expect(result.envelope.data.receipt).toEqual(
                            receipts[0]!.receipt,
                        )
                    } else {
                        expect(
                            result.status,
                            result.envelope.errorMessage,
                        ).toBe(409)
                        expect(result.envelope.success).toBe(false)
                    }
                }
                expect(receipts[0]!.receipt?.total_amount).toBe("190.00")
                for (const [index, command] of commands.entries()) {
                    const replay = await write<PublicView>(
                        endpoint,
                        command,
                        undefined,
                        "POST",
                        grants[index],
                    )
                    expect(replay.receipt).toEqual(receipts[0]!.receipt)
                    expect(replay.recipient).toEqual(recipients[0])
                    await pages[index]!.reload()
                    await expect(
                        pages[index]!.getByRole("heading", {
                            name: "选品已提交",
                            exact: true,
                        }),
                    ).toBeVisible()
                }
                const proposals = await apiGet<{
                    page: ApiPage<ProposalListRow>
                }>(salesToken, "/admin/sales-selection-proposals", {
                    booklet_id: created.id,
                    page_size: 100,
                })
                expect(proposals.page.items).toHaveLength(1)
                expect(proposals.page.items[0]!.participant_id).toBe(
                    vouchers[0]!.participant_id,
                )
            } finally {
                release()
                await Promise.allSettled(pending)
                await Promise.all(
                    pages.slice(0, 2).map((page) => page.unroute(matcher)),
                )
            }
        })

        await test.step("真实后端成功提交后丢弃浏览器响应，刷新恢复套餐与地址且原命令不重复建方案", async () => {
            const page = pages[2]!
            await page.goto(publicPath)
            grants[2] = (
                await unlockViaUi(
                    page,
                    publicToken,
                    password,
                    vouchers[1]!.voucher_code,
                )
            ).access_token
            for (const item of [firstPackage, secondPackage])
                await page
                    .locator(
                        `label[for="sales-selection-public-select-${item.item_id}"]`,
                    )
                    .click()
            const reviewed = await clickWrite<PublicView>(
                page,
                "#sales-selection-public-review",
                `/public/selection/${publicToken}/session`,
            )
            expect(reviewed.data.total_amount).toBe("130.00")
            await fillRecipient(page, recipients[1]!)
            let originalBody: Record<string, unknown> | undefined
            let committed: PublicView | undefined
            const matcher = `**${endpoint}`
            await page.route(
                matcher,
                async (route) => {
                    if (route.request().method() !== "POST")
                        return route.fallback()
                    originalBody = route.request().postDataJSON() as Record<
                        string,
                        unknown
                    >
                    // 先取得当前 shard 的真实成功结果，仅中断浏览器收到该结果。
                    const response = await route.fetch({
                        url: `${API_BASE}${endpoint}`,
                    })
                    const envelope =
                        (await response.json()) as Envelope<PublicView>
                    expect(
                        response.ok() && envelope.success !== false,
                        envelope.errorMessage,
                    ).toBe(true)
                    committed = envelope.data
                    expect(committed.kind).toBe("RECEIPT")
                    await route.abort("failed")
                },
                { times: 1 },
            )
            try {
                await page.locator("#sales-selection-public-submit").click()
                await expect(
                    page.locator("#sales-selection-public-reconcile"),
                ).toBeVisible()
            } finally {
                await page.unroute(matcher)
            }
            expect(originalBody?.recipient).toEqual(recipients[1])
            expect(committed?.recipient).toEqual(recipients[1])
            expect(committed?.receipt?.total_amount).toBe("130.00")
            expect((await publicView(publicToken, grants[2])).receipt).toEqual(
                committed?.receipt,
            )
            await page.reload()
            await expect(
                page.getByRole("heading", { name: "选品已提交", exact: true }),
            ).toBeVisible()
            await expect(page.getByLabel("收件信息")).toContainText(
                recipients[1]!.address,
            )
            expect(originalBody, "响应丢失后必须保留原提交命令").toBeTruthy()
            const replay = await write<PublicView>(
                endpoint,
                originalBody,
                undefined,
                "POST",
                grants[2],
            )
            expect(replay.kind).toBe("RECEIPT")
            expect(replay.receipt).toEqual(committed?.receipt)
            expect(replay.recipient).toEqual(recipients[1])
            receipts[1] = replay
            await assertMobileWidth(page)
        })

        await test.step("同 SKU 跨套餐保持四条来源明细，真实 XLSX 不合并成员数量或地址", async () => {
            const proposals = await apiGet<{ page: ApiPage<ProposalListRow> }>(
                salesToken,
                "/admin/sales-selection-proposals",
                { booklet_id: created.id, page_size: 100 },
            )
            expect(proposals.page.items).toHaveLength(2)
            const details = await apiGet<SelectionDetail[]>(
                salesToken,
                `/admin/sales-selection-books/${created.id}/selection-details`,
            )
            expect(details).toHaveLength(2)
            for (const [index, voucher] of vouchers.entries()) {
                const listRow = proposals.page.items.find(
                    (proposal) =>
                        proposal.participant_id === voucher.participant_id,
                )!
                expect(listRow).toBeTruthy()
                expect(listRow).not.toHaveProperty("recipient")
                const proposal = await apiGet<Proposal>(
                    salesToken,
                    `/admin/sales-selection-proposals/${listRow.id}`,
                )
                const firstQuantity = index === 0 ? 2 : 1
                const total = index === 0 ? "190.00" : "130.00"
                expect(proposal).toMatchObject({
                    form: "PACKAGE",
                    submit_mode: "PICKUP_VOUCHER",
                    participant_id: voucher.participant_id,
                    recipient: recipients[index],
                    total_amount: total,
                })
                expect(proposal.display_lines).toHaveLength(2)
                expect(proposal.display_lines).toEqual(
                    expect.arrayContaining([
                        expect.objectContaining({
                            display_item_id: firstPackage.item_id,
                            quantity: firstQuantity,
                            line_amount: index === 0 ? "120.00" : "60.00",
                        }),
                        expect.objectContaining({
                            display_item_id: secondPackage.item_id,
                            quantity: 1,
                            line_amount: "70.00",
                        }),
                    ]),
                )
                const expectedLines = [
                    {
                        display_item_id: firstPackage.item_id,
                        name: fixture.skus[0]!.name,
                        quantity: firstQuantity,
                        unit_price: "25.00",
                        line_amount: index === 0 ? "50.00" : "25.00",
                    },
                    {
                        display_item_id: firstPackage.item_id,
                        name: fixture.skus[1]!.name,
                        quantity: firstQuantity,
                        unit_price: "35.00",
                        line_amount: index === 0 ? "70.00" : "35.00",
                    },
                    {
                        display_item_id: secondPackage.item_id,
                        name: fixture.skus[0]!.name,
                        quantity: 1,
                        unit_price: "25.00",
                        line_amount: "25.00",
                    },
                    {
                        display_item_id: secondPackage.item_id,
                        name: fixture.skus[2]!.name,
                        quantity: 1,
                        unit_price: "45.00",
                        line_amount: "45.00",
                    },
                ]
                expect(proposal.sku_lines).toHaveLength(4)
                expect(proposal.sku_lines).toEqual(
                    expect.arrayContaining(
                        expectedLines.map((line) =>
                            expect.objectContaining(line),
                        ),
                    ),
                )
                const detail = details.find(
                    (row) => row.participant_id === voucher.participant_id,
                )!
                expect(detail).toMatchObject({
                    voucher_code: voucher.voucher_code,
                    proposal_id: proposal.id,
                    recipient: recipients[index],
                    total_amount: total,
                })
                expect(detail.items).toHaveLength(4)
                expect(detail.items).toEqual(
                    expect.arrayContaining(proposal.sku_lines),
                )
                expect(proposal.proposal_no).toBe(
                    receipts[index]!.receipt?.proposal_no,
                )
            }
            await salesPage.reload()
            const [download] = await Promise.all([
                salesPage.waitForEvent("download"),
                salesPage
                    .locator("#sales-selection-detail-export-selection-details")
                    .click(),
            ])
            expect(await download.failure()).toBeNull()
            const path = await download.path()
            expect(path).toBeTruthy()
            const sheet = (await readImportWorkbook(path!)).getWorksheet(
                "选品明细",
            )!
            expect(sheet).toBeTruthy()
            expect(sheet.rowCount).toBe(9)
            const rows = sheet.getRows(2, 8)!
            for (const [index, recipient] of recipients.entries()) {
                const personRows = rows.filter(
                    (row) => row.getCell(4).text === recipient.name,
                )
                expect(personRows).toHaveLength(4)
                const detail = details.find(
                    (row) =>
                        row.participant_id === vouchers[index]!.participant_id,
                )!
                for (const row of personRows) {
                    expect(row.getCell(1).text).toBe(
                        vouchers[index]!.voucher_code,
                    )
                    expect(row.getCell(2).text).toBe(
                        receipts[index]!.receipt?.proposal_no,
                    )
                    expect(
                        [5, 6, 7, 8, 9].map(
                            (column) => row.getCell(column).text,
                        ),
                    ).toEqual([
                        recipient.phone,
                        recipient.province,
                        recipient.city,
                        recipient.district,
                        recipient.address,
                    ])
                    expect(row.getCell(16).text).toBe(detail.total_amount)
                    for (const column of [1, 5, 12, 14, 15, 16])
                        expect(typeof row.getCell(column).value).toBe("string")
                }
                expect(
                    personRows
                        .map((row) =>
                            [10, 12, 14, 15]
                                .map((column) => row.getCell(column).text)
                                .join("|"),
                        )
                        .sort(),
                ).toEqual(
                    detail.items
                        .map((item) =>
                            [
                                item.name,
                                String(item.quantity),
                                item.unit_price,
                                item.line_amount,
                            ].join("|"),
                        )
                        .sort(),
                )
            }
            expect((await book(salesToken, created.id)).status).toBe(
                "PUBLISHED",
            )
            expect(
                (
                    await apiGet<Voucher[]>(
                        salesToken,
                        `/admin/sales-selection-books/${created.id}/vouchers`,
                    )
                ).every((voucher) => voucher.submitted && voucher.proposal_id),
            ).toBe(true)
        })
    } finally {
        await Promise.all(contexts.map((context) => context.close()))
    }
})
