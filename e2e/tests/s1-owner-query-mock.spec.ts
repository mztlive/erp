/**
 * S1 浏览器检查：模拟登录、人员目录、四类列表及文件任务，
 * 用 105/102/3 条数据验证同名候选人、跨页与下载。
 * 不作为真实库、组织范围或业务验收证据。
 */
import { expect, test, type FrameLocator, type Page } from "../helpers/test"
import { workPage as activeWorkPage } from "../helpers/workspace-page"

const OWNER_A = "user-a"
const OWNER_B = "user-b"
const LABEL_A = "张三（zhang-a）"
const LABEL_B = "张三（zhang-b） · 已停用"
const NOW = Math.floor(Date.now() / 1000)
const owners = [
    { value: OWNER_A, label: LABEL_A },
    { value: OWNER_B, label: LABEL_B },
]
/** 人员目录选项：姓名、账号分段拼接；非 active 才加停用标记。 */
const directoryPeople = [
    { id: OWNER_A, name: "张三", account: "zhang-a", status: "active" },
    { id: OWNER_B, name: "张三", account: "zhang-b", status: "suspended" },
]
const PICKER_LABEL_A = "张三 · zhang-a"
const PICKER_LABEL_B = "张三 · zhang-b（已停用）"

type Kind = "customers" | "contracts" | "sales" | "purchases"
type TransferRequest = { kind: string; input: { owner_user_ids?: string }; request_id: string }

function envelope(data: unknown) {
    return { status: 200, errorMessage: "", data, success: true }
}

function parseOwnerIds(url: string): string[] {
    const raw = new URL(url).searchParams.get("owner_user_ids")
    if (!raw) return []
    return [...new Set(raw.split(",").map((part) => part.trim()).filter(Boolean))]
}

function pageParams(url: string) {
    const q = new URL(url).searchParams
    return {
        page: Math.max(1, Number.parseInt(q.get("page") ?? "1", 10) || 1),
        pageSize: Math.min(
            100,
            Math.max(1, Number.parseInt(q.get("page_size") ?? "20", 10) || 20),
        ),
    }
}

function paginate<T>(items: T[], url: string) {
    const { page, pageSize } = pageParams(url)
    const start = (page - 1) * pageSize
    return {
        items: items.slice(start, start + pageSize),
        total: items.length,
        page,
        page_size: pageSize,
    }
}

function peoplePayload(url: string) {
    const params = new URL(url).searchParams
    const idQuery = params.get("ids")
    let items = directoryPeople.map((person) => ({ ...person, org_label: null }))
    if (idQuery) {
        const ids = new Set(idQuery.split(",").map((part) => part.trim()).filter(Boolean))
        items = items.filter((person) => ids.has(person.id))
    } else {
        const q = (params.get("q") ?? "").trim().toLowerCase()
        if (q) {
            items = items.filter(
                (person) =>
                    person.name.toLowerCase().includes(q) ||
                    person.account.toLowerCase().includes(q),
            )
        }
    }
    return {
        items,
        total: items.length,
        page: 1,
        page_size: 20,
        scope_version: "s1-people-v1",
        as_of: "2026-09-16T00:00:00Z",
        empty_reason: null,
    }
}

function meta() {
    return {
        empty_reason: null,
        scope_version: "s1-mock-v1",
        // 不写 policy/organization 版本。登录资料的版本通常大于 1，
        // 写死 1 会被范围缓存当成过期响应，目录成功也会显示成系统失败。
        scope_summary: "S1 mock visible owner scope",
        as_of: "2026-09-16T00:00:00Z",
        owner_options: owners,
    }
}

function makeCustomers() {
    const rows = []
    for (let i = 1; i <= 102; i += 1) {
        rows.push({
            id: `cust-a-${String(i).padStart(3, "0")}`,
            party_id: `party-a-${i}`,
            legal_name: `S1同名甲${i}`,
            customer_no: `C-A-${i}`,
            status: "active",
            owner_user_id: OWNER_A,
            owner_user_name: LABEL_A,
            collaborator_count: 0,
            scope_tags: ["assigned"],
            version: 1,
            created_at: NOW,
            updated_at: NOW,
        })
    }
    for (let i = 1; i <= 3; i += 1) {
        rows.push({
            id: `cust-b-${i}`,
            party_id: `party-b-${i}`,
            legal_name: `S1同名乙${i}`,
            customer_no: `C-B-${i}`,
            status: "active",
            owner_user_id: OWNER_B,
            owner_user_name: LABEL_B,
            collaborator_count: 0,
            scope_tags: ["assigned"],
            version: 1,
            created_at: NOW,
            updated_at: NOW,
        })
    }
    return rows
}

function makeContracts() {
    const rows = []
    for (let i = 1; i <= 102; i += 1) {
        rows.push({
            id: `ct-a-${String(i).padStart(3, "0")}`,
            contract_no: `HT-A-${i}`,
            customer_id: `cust-a-${i}`,
            customer_no: `C-A-${i}`,
            settlement_party_id: "sp-1",
            status: "EFFECTIVE",
            owner_user_id: OWNER_A,
            owner_user_name: LABEL_A,
            created_at: NOW,
            version: 1,
            current_revision: {
                id: `rev-a-${i}`,
                revision_no: 1,
                contract_pdf_file_id: "file-1",
                archive_source: "upload",
                customer_name: `客户甲${i}`,
                settlement_party_name: "结算主体",
                payment_term_code: "PERIOD_MONTH_15",
                payment_term_name: "月结15",
                invoice_type: "vat_special",
                tax_point: "invoice",
                valid_from: "2026-01-01",
                valid_to: "2026-12-31",
                signed_at: "2026-01-01",
                created_at: NOW,
            },
        })
    }
    for (let i = 1; i <= 3; i += 1) {
        rows.push({
            id: `ct-b-${i}`,
            contract_no: `HT-B-${i}`,
            customer_id: `cust-b-${i}`,
            customer_no: `C-B-${i}`,
            settlement_party_id: "sp-1",
            status: "EFFECTIVE",
            owner_user_id: OWNER_B,
            owner_user_name: LABEL_B,
            created_at: NOW,
            version: 1,
            current_revision: {
                id: `rev-b-${i}`,
                revision_no: 1,
                contract_pdf_file_id: "file-1",
                archive_source: "upload",
                customer_name: `客户乙${i}`,
                settlement_party_name: "结算主体",
                payment_term_code: "PERIOD_MONTH_15",
                payment_term_name: "月结15",
                invoice_type: "vat_special",
                tax_point: "invoice",
                valid_from: "2026-01-01",
                valid_to: "2026-12-31",
                signed_at: "2026-01-01",
                created_at: NOW,
            },
        })
    }
    return rows
}

function makeSales() {
    const rows = []
    for (let i = 1; i <= 102; i += 1) {
        rows.push({
            id: `so-a-${String(i).padStart(3, "0")}`,
            order_no: `SO-A-${i}`,
            business_type: "GOODS_SERVICE",
            origin_system: "ERP",
            customer_id: `cust-a-${i}`,
            commercial_status: "DRAFT",
            review_status: "NOT_SUBMITTED",
            fulfillment_progress: "NOT_STARTED",
            collection_progress: "NOT_COLLECTED",
            invoice_progress: "NOT_INVOICED",
            close_status: "OPEN",
            version: 1,
            created_at: NOW,
            updated_at: NOW,
            owner_user_id: OWNER_A,
            owner_user_name: LABEL_A,
            stage: { code: "draft", label: "草稿", tone: "neutral" },
        })
    }
    for (let i = 1; i <= 3; i += 1) {
        rows.push({
            id: `so-b-${i}`,
            order_no: `SO-B-${i}`,
            business_type: "GOODS_SERVICE",
            origin_system: "ERP",
            customer_id: `cust-b-${i}`,
            commercial_status: "DRAFT",
            review_status: "NOT_SUBMITTED",
            fulfillment_progress: "NOT_STARTED",
            collection_progress: "NOT_COLLECTED",
            invoice_progress: "NOT_INVOICED",
            close_status: "OPEN",
            version: 1,
            created_at: NOW,
            updated_at: NOW,
            owner_user_id: OWNER_B,
            owner_user_name: LABEL_B,
            stage: { code: "draft", label: "草稿", tone: "neutral" },
        })
    }
    return rows
}

function makePurchases() {
    const rows = []
    for (let i = 1; i <= 102; i += 1) {
        rows.push({
            id: `po-a-${String(i).padStart(3, "0")}`,
            purchase_no: `PO-A-${i}`,
            sales_order_id: `so-a-${i}`,
            sales_order_no: `SO-A-${i}`,
            supplier_id: "sup-1",
            supplier_name: "供应商甲",
            purchase_type: "PHYSICAL",
            fulfillment_responsibility: "WAREHOUSE",
            payment_term_code: "PERIOD_MONTH_15",
            owner_user_id: OWNER_A,
            owner_name: LABEL_A,
            status: "DRAFT",
            review_status: "NONE",
            gross_amount: "100.00",
            net_amount: "88.50",
            tax_amount: "11.50",
            payment_progress: "NOT_STARTED",
            invoice_progress: "NOT_STARTED",
            fulfillment_progress: "NOT_STARTED",
            version: 1,
            created_at: NOW,
        })
    }
    for (let i = 1; i <= 3; i += 1) {
        rows.push({
            id: `po-b-${i}`,
            purchase_no: `PO-B-${i}`,
            sales_order_id: `so-b-${i}`,
            sales_order_no: `SO-B-${i}`,
            supplier_id: "sup-1",
            supplier_name: "供应商乙",
            purchase_type: "PHYSICAL",
            fulfillment_responsibility: "WAREHOUSE",
            payment_term_code: "PERIOD_MONTH_15",
            owner_user_id: OWNER_B,
            owner_name: LABEL_B,
            status: "DRAFT",
            review_status: "NONE",
            gross_amount: "100.00",
            net_amount: "88.50",
            tax_amount: "11.50",
            payment_progress: "NOT_STARTED",
            invoice_progress: "NOT_STARTED",
            fulfillment_progress: "NOT_STARTED",
            version: 1,
            created_at: NOW,
        })
    }
    return rows
}

const customers = makeCustomers()
const contracts = makeContracts()
const sales = makeSales()
const purchases = makePurchases()

function filterByOwner<T extends Record<string, unknown>>(
    items: T[],
    url: string,
    field: string,
) {
    const ids = parseOwnerIds(url)
    if (ids.length === 0) return items
    return items.filter((item) => ids.includes(String(item[field] ?? "")))
}

function payload(kind: Kind, url: string) {
    if (kind === "customers") {
        const filtered = filterByOwner(customers, url, "owner_user_id")
        return { ...paginate(filtered, url), ...meta(), ownership_basis: "customer_assignment_owner" }
    }
    if (kind === "contracts") {
        const filtered = filterByOwner(contracts, url, "owner_user_id")
        return {
            ...paginate(filtered, url),
            ...meta(),
            ownership_basis: "customer_current_owner",
            metrics: {
                all: filtered.length,
                effective: filtered.length,
                expiring_30d: 0,
                expired: 0,
                terminated: 0,
            },
            settlement_options: [{ value: "sp-1", label: "结算主体" }],
        }
    }
    if (kind === "sales") {
        const filtered = filterByOwner(sales, url, "owner_user_id")
        return { ...paginate(filtered, url), ...meta(), ownership_basis: "document_sales_owner" }
    }
    const filtered = filterByOwner(purchases, url, "owner_user_id")
    return { ...paginate(filtered, url), ...meta(), ownership_basis: "purchase_owner" }
}

async function installMocks(page: Page, requests: { kind: Kind; url: string; ownerIds: string[]; page: number; pageSize: number }[]) {
    const fulfill = async (kind: Kind, route: { request: () => { url: () => string; method: () => string }; fulfill: (r: Record<string, unknown>) => Promise<void> }) => {
        const url = route.request().url()
        const cors = {
            "access-control-allow-origin": "*",
            "access-control-allow-headers": "*",
            "access-control-allow-methods": "GET,OPTIONS",
        }
        if (route.request().method() === "OPTIONS") {
            await route.fulfill({ status: 204, headers: cors })
            return
        }
        requests.push({ kind, url, ownerIds: parseOwnerIds(url), ...pageParams(url) })
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            headers: cors,
            body: JSON.stringify(envelope(payload(kind, url))),
        })
    }
    const context = page.context()
    await context.route(/\/admin\/customers(\/all-authorized)?(\?|$)/, (route) => fulfill("customers", route))
    await context.route(/\/admin\/contracts(\?|$)/, (route) => fulfill("contracts", route))
    await context.route(/\/admin\/sales-orders(\?|$)/, (route) => fulfill("sales", route))
    await context.route(/\/admin\/purchase-orders(\?|$)/, (route) => fulfill("purchases", route))
    await context.route(/\/admin\/(?:salespeople|procurement-people|business-people)(?:\/selected)?(?:\?|$)/, async (route) => {
        const cors = {
            "access-control-allow-origin": "*",
            "access-control-allow-headers": "*",
            "access-control-allow-methods": "GET,OPTIONS",
        }
        if (route.request().method() === "OPTIONS") {
            await route.fulfill({ status: 204, headers: cors })
            return
        }
        await route.fulfill({
            status: 200,
            contentType: "application/json",
            headers: cors,
            body: JSON.stringify(envelope(peoplePayload(route.request().url()))),
        })
    })
    await context.route(/\/admin\/org-units(?:\?|$)/, async (route) => {
        await fulfillJson(route, {
            version: 1,
            units: [],
            memberships: [],
            management: [],
            people: [],
            roles: [],
            scope_version: "s1-org-v1",
            as_of: "2026-09-16T00:00:00Z",
            empty_reason: null,
            scope_summary: "S1 mock org",
            ownership_basis: "org_unit",
        })
    })
    await context.route(/\/admin\/settlement-parties(?:\/selected)?(?:\?|$)/, async (route) => {
        const { page, pageSize } = pageParams(route.request().url())
        await fulfillJson(route, {
            items: [],
            total: 0,
            page,
            page_size: pageSize,
            scope_version: "s1-parties-v1",
            as_of: "2026-09-16T00:00:00Z",
            empty_reason: null,
            scope_summary: "S1 mock settlement parties",
            ownership_basis: "settlement_party",
        })
    })
}

async function fulfillJson(
    route: {
        request: () => { method: () => string }
        fulfill: (response: Record<string, unknown>) => Promise<void>
    },
    data: unknown,
) {
    const cors = {
        "access-control-allow-origin": "*",
        "access-control-allow-headers": "*",
        "access-control-allow-methods": "GET,POST,OPTIONS",
    }
    if (route.request().method() === "OPTIONS") {
        await route.fulfill({ status: 204, headers: cors })
        return
    }
    await route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: cors,
        body: JSON.stringify(envelope(data)),
    })
}

async function selectOwner(workPage: FrameLocator | Page, inputId: string, userId: string) {
    await workPage.locator(`#${inputId}`).click()
    const optionA = workPage.locator(`#${inputId}-option-${OWNER_A}`)
    const optionB = workPage.locator(`#${inputId}-option-${OWNER_B}`)
    await expect(optionA).toBeVisible()
    await expect(optionB).toBeVisible()
    await expect(optionA.getByText(PICKER_LABEL_A, { exact: true })).toBeVisible()
    await expect(optionB.getByText(PICKER_LABEL_B, { exact: true })).toBeVisible()
    await workPage.locator(`#${inputId}-option-${userId}`).click()
    await workPage.locator(`#${inputId}`).press("Escape")
}

test("S1 mock API owner filter, export and 390px", async ({ page }) => {
    const requests: { kind: Kind; url: string; ownerIds: string[]; page: number; pageSize: number }[] = []
    const context = page.context()
    const apiOrigin = new URL(process.env.ERP_E2E_SOURCE_API_BASE ?? "http://127.0.0.1:10001").origin
    await context.route(`${apiOrigin}/**`, (route) => route.abort())
    await context.route(`${apiOrigin}/login`, (route) => fulfillJson(route, { token: "s1-browser-token" }))
    await context.route(`${apiOrigin}/account/profile`, (route) => fulfillJson(route, {
        userid: "s1-user",
        account: "s1-user",
        name: "S1 测试账号",
        subject: "user:admin:s1-user",
        account_kind: "admin",
        role_ids: [],
        permissions: [
            "customer:list", "contract:list", "sales_order:list", "purchase_order:list",
            "sales_person:list", "procurement_person:list", "org_unit:list",
        ],
    }))
    await installMocks(page, requests)
    await page.goto("/login?returnTo=%2Fsales%2Fcustomers")
    await page.locator("#governance-auth-login-account").fill("s1-user")
    await page.locator("#governance-auth-login-password").fill("s1-password")
    await page.locator("#governance-auth-login-submit").click()
    await expect(page).toHaveURL(/\/sales\/customers$/)

    const workPage = activeWorkPage(page)

    const cases: Array<{
        name: string
        kind: Kind
        path: string
        ownerId: string
        applyId: string
        tableId: string
        count3: string
        ready: string
        marker: string
    }> = [
        {
            name: "customers",
            kind: "customers",
            path: "/sales/customers",
            ownerId: "customers-directory-owner",
            applyId: "#customers-directory-query",
            tableId: "customers-directory-table",
            count3: "共 3 个客户",
            ready: "共 105 个客户",
            marker: "S1同名甲1",
        },
        {
            name: "contracts",
            kind: "contracts",
            path: "/sales/contracts",
            ownerId: "card-contracts-list-filter-owner",
            applyId: "#card-contracts-list-apply-filters",
            tableId: "card-contracts-list-table",
            count3: "共 3 份合同",
            ready: "共 105 份合同",
            marker: "HT-A-1",
        },
        {
            name: "sales",
            kind: "sales",
            path: "/sales/orders",
            ownerId: "sales-orders-list-owner",
            applyId: "#sales-orders-list-filter-apply",
            tableId: "sales-orders-list-table",
            count3: "共 3 张销售单",
            ready: "共 105 张销售单",
            marker: "SO-A-1",
        },
        {
            name: "purchases",
            kind: "purchases",
            path: "/procurement/orders",
            ownerId: "procurement-orders-list-owner",
            applyId: "#procurement-orders-list-apply-filters",
            tableId: "procurement-orders-list-table",
            count3: "共 3 张采购单",
            ready: "共 105 张采购单",
            marker: "PO-A-1",
        },
    ]

    for (const spec of cases) {
        requests.length = 0
        await page.goto(spec.path)
        await expect(workPage.locator(`#${spec.tableId} [data-row-id]`).first()).toBeVisible({ timeout: 20_000 })
        await expect(workPage.getByText(spec.marker).first()).toBeVisible()
        await expect(workPage.getByText(spec.ready).first()).toBeVisible()
        expect(requests.some((item) => item.kind === spec.kind), `${spec.name} mock hit`).toBeTruthy()
        await selectOwner(workPage, spec.ownerId, OWNER_B)
        await workPage.locator(spec.applyId).click()
        await expect(workPage.getByRole("status").filter({ hasText: spec.count3 }).first()).toBeVisible({ timeout: 20_000 })
        const req = [...requests].reverse().find((item) => item.kind === spec.kind)
        expect(req?.ownerIds, spec.name).toEqual([OWNER_B])
        await expect(workPage.locator(`#${spec.tableId} [data-row-id]`)).toHaveCount(3)
        expect(new URL(page.url()).searchParams.get("ownerUserIds")).toBe(OWNER_B)
    }

    requests.length = 0
    await page.goto("/sales/orders")
    await expect(workPage.getByRole("status").filter({ hasText: "共 105 张销售单" }).first()).toBeVisible({ timeout: 20_000 })
    await selectOwner(workPage, "sales-orders-list-owner", OWNER_A)
    await workPage.locator("#sales-orders-list-filter-apply").click()
    await expect(workPage.getByRole("status").filter({ hasText: "共 102 张销售单" }).first()).toBeVisible()
    await page.reload()
    await expect(workPage.getByRole("status").filter({ hasText: "共 102 张销售单" }).first()).toBeVisible()
    expect(new URL(page.url()).searchParams.get("ownerUserIds")).toBe(OWNER_A)
    const page6 = new URL(page.url())
    page6.searchParams.set("page", "6")
    await page.goto(`${page6.pathname}?${page6.searchParams.toString()}`)
    await expect(workPage.getByRole("status").filter({ hasText: "共 102 张销售单" }).first()).toBeVisible()
    await expect(workPage.locator("#sales-orders-list-table [data-row-id]")).toHaveCount(2)

    const transfers: TransferRequest[] = []
    const transferId = "s1-sales-export"
    const fileName = "S1销售单.csv"
    const job = { id: transferId, job_no: "S1-EXPORT", status: "succeeded", result_file_asset_id: "s1-file" }
    const exportedOrders = sales.filter((row) => row.owner_user_id === OWNER_A)
    const csv = ["销售单号", ...exportedOrders.map((row) => row.order_no)].join("\n")
    await page.context().route(/\/account\/data-transfers(?:\?.*)?$/, async (route) => {
        if (route.request().method() === "POST") {
            transfers.push(route.request().postDataJSON() as TransferRequest)
        }
        await fulfillJson(route, job)
    })
    await page.context().route(`**/account/data-transfers/${transferId}`, (route) => fulfillJson(route, job))
    await page.context().route(`**/account/data-transfers/${transferId}/result`, (route) =>
        fulfillJson(route, {
            file_name: fileName,
            content_type: "text/csv; charset=utf-8",
            content_base64: Buffer.from(csv).toString("base64"),
            metadata: { row_count: exportedOrders.length },
        }),
    )
    const [download] = await Promise.all([
        page.waitForEvent("download"),
        workPage.locator("#sales-orders-list-header-export").click(),
    ])
    expect(transfers).toEqual([expect.objectContaining({
        kind: "sales_orders",
        input: expect.objectContaining({ owner_user_ids: OWNER_A }),
        request_id: expect.any(String),
    })])
    expect(download.suggestedFilename()).toBe(fileName)
    const stream = await download.createReadStream()
    const chunks: Buffer[] = []
    for await (const chunk of stream) chunks.push(chunk)
    const content = Buffer.concat(chunks).toString("utf8")
    expect(content).toBe(csv)
    await expect(workPage.getByText(/已生成 CSV 文件，共 102 行/)).toBeVisible()

    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(`/sales/orders?ownerUserIds=${OWNER_B}`)
    await expect(workPage.getByRole("status").filter({ hasText: "共 3 张销售单" }).first()).toBeVisible()
    await expect(workPage.getByText("负责销售：已选 1 人")).toBeVisible()
    await workPage.locator("#sales-orders-list-owner").click()
    await expect(workPage.locator(`#sales-orders-list-owner-option-${OWNER_B}`)).toBeVisible()
    await page.keyboard.press("Escape")
    const salesOverflow = await workPage.locator("html").evaluate((element) => ({
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
    }))
    expect(salesOverflow.scrollWidth).toBeLessThanOrEqual(salesOverflow.clientWidth + 1)

    await page.goto(`/procurement/orders?ownerUserIds=${OWNER_B}`)
    await expect(workPage.getByRole("status").filter({ hasText: "共 3 张采购单" }).first()).toBeVisible()
    await expect(workPage.getByText("采购负责人：已选").first()).toBeVisible()
    await workPage.locator("#procurement-orders-list-owner").scrollIntoViewIfNeeded()
    await workPage.locator("#procurement-orders-list-owner").click()
    const purchaseOverflow = await workPage.locator("html").evaluate((element) => ({
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
    }))
    expect(purchaseOverflow.scrollWidth).toBeLessThanOrEqual(purchaseOverflow.clientWidth + 1)
})
