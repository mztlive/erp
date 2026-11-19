/**
 * [flow-28] 供应商供给批量调价、可供更新与中断恢复。
 * 合同：docs/supplier-offering-batch-contract.md。
 * 真实 API 验证预检零写入和命令回放；页面验证粘贴、覆盖确认、冲突重读与会话恢复。
 * 响应丢失场景先调用本次隔离 API 完成写入，再丢弃响应；业务结果不使用 mock。
 */
import { workPage } from "../helpers/workspace-page"
import { test, expect, type APIResponse, type Locator, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { openLoggedInWorkspace } from "../helpers/login"
import { chooseOption } from "../helpers/ui"

type ApiPage<T> = { items: T[]; total: number }
type Identity = {
    id: string
    category_code?: string
    brand_code?: string
    unit_code?: string
    supplier_no?: string
    name?: string
}
type Sku = { id: string; sku_no: string; name: string }
type Offering = {
    id: string
    sku_id: string
    supplier_sku_code: string
    source_type: string
    current_revision_id: string
    current_revision_no: number
    dropship_supply_price_gross: string
    bulk_supply_price_gross: string
    input_tax_rate: string
    availability_status: string
    availability_version: number
    available_quantity: string | null
}
type Terms = {
    dropship_supply_price_gross: string
    bulk_supply_price_gross: string
    input_tax_rate: string
    bulk_minimum_order_quantity: string
    product_capabilities: string[]
    valid_from: string
}
type BatchRow = { row_id: string; input: Record<string, unknown> }
type BatchResult = {
    rows: Array<{
        row_id: string
        status: string
        message: string | null
        result: { offering_id: string; revision_id?: string; revision_no?: number } | null
    }>
}
type BatchMode = "revise" | "availability"
type BatchBody = { rows: BatchRow[]; validate_only: boolean }
type TransferBody = { kind: string; input: BatchBody & { mode: BatchMode }; request_id: string }
type TransferJob = { id: string; result_file_asset_id: string | null }
type TransferFile = { content_base64: string }
type Envelope<T> = { success?: boolean; errorMessage?: string; data: T }
const VISIBLE = { timeout: 20_000 }
const TRANSFERS = "/account/data-transfers"

function businessDate(): string {
    return new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai", year: "numeric", month: "2-digit", day: "2-digit",
    }).format(new Date())
}

function terms(dropship = "21.00", bulk = "19.00"): Terms {
    return {
        dropship_supply_price_gross: dropship,
        bulk_supply_price_gross: bulk,
        input_tax_rate: "0.09",
        bulk_minimum_order_quantity: "5",
        product_capabilities: [],
        valid_from: businessDate(),
    }
}

async function postRaw<T>(token: string, endpoint: string, body: unknown) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    return { response, envelope: await response.json() as Envelope<T> }
}

async function post<T>(token: string, endpoint: string, body: unknown): Promise<T> {
    const { response, envelope } = await postRaw<T>(token, endpoint, body)
    expect(response.ok && envelope.success !== false, `${endpoint}: ${envelope.errorMessage ?? response.status}`).toBe(true)
    expect(envelope.data, `${endpoint} 缺少 data`).toBeTruthy()
    return envelope.data
}

async function batch(token: string, mode: "revise" | "availability", rows: BatchRow[], validateOnly = false) {
    return post<BatchResult>(token, `/admin/supplier-offerings/batch/${mode}`, { rows, validate_only: validateOnly })
}

function expectStatuses(result: BatchResult, rows: BatchRow[], statuses: string[]) {
    expect(result.rows.map((row) => row.row_id)).toEqual(rows.map((row) => row.row_id))
    expect(result.rows.map((row) => row.status)).toEqual(statuses)
}

async function prepareSkus(token: string, prefix: string) {
    const [categories, brands, units, suppliers, admins] = await Promise.all([
        apiGet<ApiPage<Identity>>(token, "/admin/product-categories", { category_code: "TEA", page_size: 100 }),
        apiGet<ApiPage<Identity>>(token, "/admin/product-brands", { brand_code: "SF", page_size: 100 }),
        apiGet<ApiPage<Identity>>(token, "/admin/unit-of-measures", { unit_code: "HE", page_size: 100 }),
        apiGet<ApiPage<Identity>>(token, "/admin/suppliers", { page_size: 100 }),
        apiGet<Array<{ id: string; account: string }>>(token, "/admin/admins"),
    ])
    const category = categories.items.find((item) => item.category_code === "TEA")
    const brand = brands.items.find((item) => item.brand_code === "SF")
    const unit = units.items.find((item) => item.unit_code === "HE")
    const supplier = suppliers.items.find((item) => item.supplier_no === "SUP-HZSF")
    const owner = admins.find((item) => item.account === "caigou")
    expect(category && brand && unit && supplier && owner, "批量验收需要固定目录、有效实物供应商及采购岗位种子").toBeTruthy()
    const skus: Sku[] = []
    for (const suffix of ["A", "B"]) {
        const code = `${prefix}-${suffix}`
        const product = await post<{ id: string }>(token, "/admin/products", {
            change_reason: "E2E 供给批量独立商品",
            product_no: code,
            product_kind: "PHYSICAL",
            maintainer_user_id: owner!.id,
            name: `E2E 批量供给 ${code}`,
            category_id: category!.id,
            brand_id: brand!.id,
            status: "active",
            effective_from: businessDate(),
            carousel_media: [],
            detail_media: [],
            skus: [{ sku_no: code, name: `E2E 批量供给 ${code}`, base_unit_id: unit!.id, sales_visible_price_gross: "99.00", spec_entries: [] }],
        })
        const result = await apiGet<ApiPage<Sku>>(token, `/admin/products/${product.id}/skus`, { page_size: 100 })
        expect(result.items).toHaveLength(1)
        skus.push(result.items[0]!)
    }
    return { skus, supplier: supplier! }
}

async function offerings(token: string, prefix: string): Promise<Offering[]> {
    return (await apiGet<ApiPage<Offering>>(token, "/admin/supplier-offerings", { q: prefix, page_size: 100 })).items
        .sort((a, b) => a.supplier_sku_code.localeCompare(b.supplier_sku_code))
}

async function details(token: string, rows: Offering[]): Promise<Offering[]> {
    return Promise.all(rows.map((row) => apiGet<Offering>(token, `/admin/supplier-offerings/${row.id}`)))
}

async function history(token: string, id: string) {
    return apiGet<{ items: Array<{ id: string; revision_no: number }>; next_before_revision_no: number | null }>(token, `/admin/supplier-offerings/${id}/revisions`)
}

async function createOfferings(token: string, fixture: Awaited<ReturnType<typeof prepareSkus>>, prefix: string) {
    for (const [index, sku] of fixture.skus.entries()) {
        await post(token, "/admin/supplier-offerings", {
            sku_id: sku.id,
            supplier_id: fixture.supplier.id,
            supplier_sku_code: `000${prefix}-${index}`,
            source_type: "MANUAL",
            terms: terms(index === 0 ? "17.00" : "19.00", "15.00"),
            availability_status: "AVAILABLE",
            available_quantity: index === 0 ? "0" : null,
            change_reason: "E2E 批量维护预置供给",
            idempotency_key: `${prefix}-create-${index}`,
        })
    }
    const initial = await offerings(token, prefix)
    expect(initial.map((row) => row.supplier_sku_code)).toEqual(fixture.skus.map((_, index) => `000${prefix}-${index}`))
    expect(initial.map((row) => row.available_quantity)).toEqual(["0", null])
    expect(initial.map((row) => row.current_revision_no)).toEqual([1, 1])
    return details(token, initial)
}

function command(row: BatchRow): Record<string, unknown> {
    return row.input.command as Record<string, unknown>
}

test("批量 API：整批预检阻断、容器限制、调价与可供命令回放及版本冲突", async () => {
    const prefix = `E2E-BATCH-API-${Date.now().toString(36).toUpperCase()}`
    const setupToken = await apiToken("admin")
    const token = await apiToken("caigou")
    const fixture = await prepareSkus(setupToken, prefix)
    const initial = await createOfferings(setupToken, fixture, prefix)
    const reviseRows: BatchRow[] = initial.map((offering, index) => ({
        row_id: offering.id,
        input: { offering_id: offering.id, command: {
            expected_revision_no: offering.current_revision_no,
            terms: terms("26.00", "24.00"),
            change_reason: "E2E 批量调价",
            idempotency_key: `${prefix}-revise-${index}`,
        } },
    }))
    await test.step("非法条款阻止整批，合法预检与非法容器均不写入", async () => {
        const invalid = structuredClone(reviseRows)
        command(invalid[1]!).terms = terms("-1.00", "24.00")
        for (const validateOnly of [true, false]) {
            const result = await batch(token, "revise", invalid, validateOnly)
            expectStatuses(result, invalid, ["READY", "INVALID"])
            expect(result.rows[1]!.message).toBeTruthy()
            expect(await details(token, initial)).toEqual(initial)
        }
        expectStatuses(await batch(token, "revise", reviseRows, true), reviseRows, ["READY", "READY"])
        const [first, second] = reviseRows as [BatchRow, BatchRow]
        const cases: Array<{ rows: BatchRow[]; message: RegExp }> = [
            { rows: [], message: /1–100/ },
            { rows: Array.from({ length: 101 }, () => first), message: /1–100/ },
            { rows: [first, { ...second, row_id: first.row_id }], message: /行标识/ },
            { rows: [first, { ...second, input: { ...second.input, command: { ...command(second), idempotency_key: command(first).idempotency_key } } }], message: /提交标识/ },
            { rows: [first, { ...second, input: { ...second.input, offering_id: first.input.offering_id } }], message: /重复的供给/ },
        ]
        for (const item of cases) {
            const result = await postRaw<BatchResult>(token, "/admin/supplier-offerings/batch/revise", { rows: item.rows, validate_only: false })
            expect(result.envelope.success).toBe(false)
            expect(result.envelope.errorMessage).toMatch(item.message)
        }
        expect(await details(token, initial)).toEqual(initial)
    })
    await test.step("条款版本冲突阻止整批，调价成功后保持数量，回放不追加版本", async () => {
        const stale = structuredClone(reviseRows)
        stale[0]!.input.command = { ...(stale[0]!.input.command as Record<string, unknown>), expected_revision_no: 0 }
        expectStatuses(await batch(token, "revise", stale), stale, ["INVALID", "READY"])
        expect(await details(token, initial)).toEqual(initial)
        const revised = await batch(token, "revise", reviseRows)
        expectStatuses(revised, reviseRows, ["SUCCEEDED", "SUCCEEDED"])
        const after = await details(token, initial)
        for (let index = 0; index < after.length; index += 1) {
            expect(after[index]).toMatchObject({ current_revision_no: 2, dropship_supply_price_gross: "26.00", bulk_supply_price_gross: "24.00", available_quantity: initial[index]!.available_quantity, availability_version: initial[index]!.availability_version })
            expect((await history(token, after[index]!.id)).items.map((item) => item.revision_no)).toEqual([2, 1])
        }
        expect(await batch(token, "revise", reviseRows, true)).toEqual(revised)
        expect(await batch(token, "revise", reviseRows)).toEqual(revised)
        const changed = structuredClone(reviseRows)
        command(changed[0]!).terms = terms("27.00", "24.00")
        expectStatuses(await batch(token, "revise", changed), changed, ["INVALID", "SUCCEEDED"])
        expect(await details(token, initial)).toEqual(after)
    })

    const beforeAvailability = await details(token, initial)
    const availabilityRows: BatchRow[] = beforeAvailability.map((offering, index) => ({
        row_id: offering.id,
        input: { offering_id: offering.id, command: {
            expected_version: offering.availability_version,
            availability_status: "UNAVAILABLE",
            available_quantity: index === 0 ? null : "0",
            change_reason: "E2E 批量更新可供情况",
            idempotency_key: `${prefix}-availability-${index}`,
        } },
    }))
    await test.step("缺少或过期可供版本阻止整批，最新版本重试保留商业条款", async () => {
        const missing = structuredClone(availabilityRows)
        const command = missing[0]!.input.command as Record<string, unknown>
        delete command.expected_version
        expectStatuses(await batch(token, "availability", missing), missing, ["INVALID", "READY"])
        const stale = structuredClone(availabilityRows)
        stale[0]!.input.command = { ...(stale[0]!.input.command as Record<string, unknown>), expected_version: beforeAvailability[0]!.availability_version - 1 }
        expectStatuses(await batch(token, "availability", stale), stale, ["INVALID", "READY"])
        expect(await details(token, initial)).toEqual(beforeAvailability)
        const updated = await batch(token, "availability", availabilityRows)
        expectStatuses(updated, availabilityRows, ["SUCCEEDED", "SUCCEEDED"])
        const after = await details(token, initial)
        expect(after.map((item) => item.available_quantity)).toEqual([null, "0"])
        for (let index = 0; index < after.length; index += 1) {
            expect(after[index]).toMatchObject({ current_revision_id: beforeAvailability[index]!.current_revision_id, current_revision_no: 2, dropship_supply_price_gross: "26.00", availability_status: "UNAVAILABLE", availability_version: beforeAvailability[index]!.availability_version + 1 })
            expect((await history(token, after[index]!.id)).items.map((item) => item.revision_no)).toEqual([2, 1])
        }
        expect(await batch(token, "availability", availabilityRows, true)).toEqual(updated)
        expect(await batch(token, "availability", availabilityRows)).toEqual(updated)
        expect(await details(token, initial)).toEqual(after)
    })

    await test.step("相同客户端命令键按操作人隔离，各主体回放自己的原回执", async () => {
        const current = await details(token, initial)
        const otherActor = structuredClone(availabilityRows.slice(0, 1))
        otherActor[0]!.input.command = {
            ...(otherActor[0]!.input.command as Record<string, unknown>),
            expected_version: current[0]!.availability_version,
            available_quantity: "42",
            change_reason: "E2E 另一操作人复用相同客户端命令键",
        }
        const updated = await batch(setupToken, "availability", otherActor)
        expectStatuses(updated, otherActor, ["SUCCEEDED"])
        const after = await details(token, initial)
        expect(after[0]).toMatchObject({ available_quantity: "42", availability_version: current[0]!.availability_version + 1, current_revision_id: current[0]!.current_revision_id })
        expect(await batch(setupToken, "availability", otherActor)).toEqual(updated)
        expectStatuses(await batch(token, "availability", availabilityRows), availabilityRows, ["SUCCEEDED", "SUCCEEDED"])
        expect(await details(token, initial)).toEqual(after)
    })
})

async function readUiData<T>(response: Response | APIResponse): Promise<T> {
    const envelope = await response.json() as Envelope<T>
    expect(response.ok() && envelope.success !== false, envelope.errorMessage ?? response.statusText()).toBe(true)
    return envelope.data
}

async function clickValidate(page: Page, dialog: Locator, mode: BatchMode) {
    const [response] = await Promise.all([
        page.waitForResponse((item) => item.request().method() === "POST" && new URL(item.url()).pathname === `/admin/supplier-offerings/batch/${mode}`, VISIBLE),
        dialog.locator("#batch-supply-validate").click(),
    ])
    return { body: response.request().postDataJSON() as BatchBody, result: await readUiData<BatchResult>(response) }
}

async function waitForSupplyTransfer(page: Page, mode: BatchMode) {
    const response = await page.waitForResponse((item) => {
        if (item.request().method() !== "POST" || new URL(item.url()).pathname !== TRANSFERS) return false
        const body = item.request().postDataJSON() as TransferBody
        return body.kind === "supply_batch" && body.input.mode === mode
    }, VISIBLE)
    const body = response.request().postDataJSON() as TransferBody
    expect(body.input.validate_only).toBe(false)
    return { body, job: await readUiData<TransferJob>(response) }
}

async function readTransferBatch(response: Response | APIResponse): Promise<BatchResult> {
    const file = await readUiData<TransferFile>(response)
    return JSON.parse(Buffer.from(file.content_base64, "base64").toString("utf8")) as BatchResult
}

async function clickSupplyTransfer(page: Page, dialog: Locator, mode: BatchMode, control: string) {
    const submitted = waitForSupplyTransfer(page, mode)
    const [transfer, response] = await Promise.all([
        submitted,
        page.waitForResponse(async (item) => {
            const path = new URL(item.url()).pathname
            if (item.request().method() !== "GET" || !path.startsWith(`${TRANSFERS}/`) || !path.endsWith("/result")) return false
            const { job } = await submitted
            return path === `${TRANSFERS}/${job.id}/result`
        }, { timeout: 45_000 }),
        dialog.locator(control).click(),
    ])
    return { ...transfer, result: await readTransferBatch(response) }
}

async function selectOfferings(page: Page, prefix: string, ids: string[]) {
    await page.goto(`/procurement/supplier-offerings?q=${encodeURIComponent(prefix)}`)
    for (const id of ids) {
        const row = workPage(page).locator(`tr[data-row-id="${id}"]`)
        await expect(row).toBeVisible(VISIBLE)
        const selector = row.getByRole("checkbox")
        await selector.check()
        await expect(selector).toBeChecked()
    }
}

test("批量页面：多格粘贴、公共设置、冲突重读及真实提交响应丢失后恢复", async ({ browser }) => {
    const prefix = `E2E-BATCH-UI-${Date.now().toString(36).toUpperCase()}`
    const setupToken = await apiToken("admin")
    const fixture = await prepareSkus(setupToken, prefix)
    const initial = await createOfferings(setupToken, fixture, prefix)
    const { page } = await openLoggedInWorkspace(browser, "caigou")
    const token = await apiToken("caigou")
    expect((await offerings(token, prefix)).map((item) => item.id), "采购账号应能查询测试供给").toEqual(
        initial.map((item) => item.id),
    )
    await selectOfferings(page, prefix, initial.map((item) => item.id))
    await workPage(page).locator("#supplier-offerings-batch-revise").click()
    const revise = workPage(page).getByRole("dialog", { name: "批量调价与条款", exact: true })
    await expect(revise).toBeVisible(VISIBLE)

    await test.step("窄窗口内标题和提交区保持可见，宽表只在表格容器滚动", async () => {
        const desktop = page.viewportSize() ?? { width: 1440, height: 900 }
        await page.setViewportSize({ width: 390, height: 844 })
        await expect(revise.getByRole("heading", { name: "批量调价与条款", exact: true })).toBeInViewport()
        await expect(revise.locator("#batch-supply-submit")).toBeInViewport()
        await expect.poll(async () => {
            const bounds = await revise.boundingBox()
            return bounds != null && bounds.x >= 0 && bounds.x + bounds.width <= 391
        }).toBe(true)
        const tableOverflow = await revise.locator('[data-slot="table-container"]').evaluate((node) => ({
            width: node.clientWidth,
            contentWidth: node.scrollWidth,
            overflowX: getComputedStyle(node).overflowX,
        }))
        expect(tableOverflow.contentWidth).toBeGreaterThan(tableOverflow.width)
        expect(tableOverflow.overflowX).toBe("auto")
        const dialogOverflow = await revise.evaluate((node) => node.scrollWidth - node.clientWidth)
        expect(dialogOverflow).toBeLessThanOrEqual(1)
        await page.setViewportSize(desktop)
    })

    await test.step("TSV 多格粘贴并拒绝溢出，不改写供给订货编码或正式条款", async () => {
        const first = revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })
        const paste = (text: string) => first.evaluate((input, content) => {
            const clipboard = new DataTransfer()
            clipboard.setData("text/plain", content)
            input.dispatchEvent(new ClipboardEvent("paste", { bubbles: true, cancelable: true, clipboardData: clipboard }))
        }, text)
        await paste('"17.00"\t"15.00"\n"19.00"\t"15.00"')
        await expect(revise.getByRole("status")).toHaveText("已粘贴 2 行，请核对后校验。")
        await paste("90.00\n91.00\n92.00")
        await expect(revise.getByRole("status")).toContainText("粘贴范围超出表格")
        for (const [index, sku] of fixture.skus.entries()) {
            await expect(revise.getByLabel(`${sku.sku_no} 代发含税价`, { exact: true })).toHaveValue(index === 0 ? "17.00" : "19.00")
            await expect(revise.getByLabel(`${sku.sku_no} 集采含税价`, { exact: true })).toHaveValue("15.00")
            await expect(revise.getByText(`订货码：${initial[index]!.supplier_sku_code}`, { exact: true })).toBeVisible()
        }
        expect(await details(token, initial)).toEqual(initial)
    })

    await test.step("公共补齐保留已有值，覆盖必须确认；提交前预检无写入", async () => {
        await revise.locator("#batch-supply-common-dropshipprice").fill("22.00")
        await revise.locator("#batch-supply-common-bulkprice").fill("20.00")
        await revise.getByLabel(`${fixture.skus[1]!.sku_no} 集采含税价`, { exact: true }).fill("")
        await revise.locator("#batch-supply-fill-empty").click()
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })).toHaveValue("17.00")
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("15.00")
        await expect(revise.getByLabel(`${fixture.skus[1]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("20.00")
        await revise.locator("#batch-supply-overwrite").click()
        await expect(revise.locator("#batch-supply-overwrite-confirm")).toBeVisible()
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })).toHaveValue("17.00")
        await revise.locator("#batch-supply-overwrite-confirm").click()
        for (const sku of fixture.skus) {
            await expect(revise.getByLabel(`${sku.sku_no} 代发含税价`, { exact: true })).toHaveValue("22.00")
            await expect(revise.getByLabel(`${sku.sku_no} 集采含税价`, { exact: true })).toHaveValue("20.00")
        }
        await expect(revise.locator("#batch-supply-submit")).toHaveText("校验并提交 2 行")
        const validated = await clickValidate(page, revise, "revise")
        expect(validated.body.validate_only).toBe(true)
        expectStatuses(validated.result, validated.body.rows, ["READY", "READY"])
        expect(await details(token, initial)).toEqual(initial)
    })

    await test.step("外部修改导致版本冲突；重新读取替换完整最新条款后再次修改提交", async () => {
        await revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true }).fill("99.00")
        const externalTerms = { ...terms("34.00", "33.00"), input_tax_rate: "0.13" }
        await post(token, `/admin/supplier-offerings/${initial[0]!.id}/revisions`, {
            expected_revision_no: initial[0]!.current_revision_no,
            terms: externalTerms,
            change_reason: "E2E 模拟其他会话修改最新条款",
            idempotency_key: `${prefix}-external-revise`,
        })
        const conflict = await clickSupplyTransfer(page, revise, "revise", "#batch-supply-submit")
        const staleRow = conflict.result.rows.find((row) => row.row_id === initial[0]!.id)
        expect(staleRow).toMatchObject({ status: "INVALID", message: expect.stringContaining("版本已经变化") })
        expect(conflict.result.rows.find((row) => row.row_id === initial[1]!.id)?.status).toBe("READY")
        const blocked = await details(token, initial)
        expect(blocked.map((item) => item.current_revision_no)).toEqual([2, 1])
        await expect(revise.getByText(staleRow!.message!, { exact: true })).toBeVisible()
        const stalePrice = revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })
        await revise.locator("#batch-supply-reload-failed").click()
        await expect(revise.locator("#batch-supply-reload-confirm")).toBeVisible()
        await expect(stalePrice).toHaveValue("99.00")
        await revise.locator("#batch-supply-reload-confirm").click()
        await expect(stalePrice).toHaveValue("34.00", VISIBLE)
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("33.00")
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 税率 %`, { exact: true })).toHaveValue("13")
        await expect(revise.getByLabel(`${fixture.skus[1]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("20.00")
        for (const [index, sku] of fixture.skus.entries()) {
            await revise.getByLabel(`${sku.sku_no} 代发含税价`, { exact: true }).fill(index === 0 ? "35.00" : "36.00")
        }
        const saved = await clickSupplyTransfer(page, revise, "revise", "#batch-supply-submit")
        expectStatuses(saved.result, saved.body.input.rows, ["SUCCEEDED", "SUCCEEDED"])
        expect(command(saved.body.input.rows.find((row) => row.row_id === initial[0]!.id)!).expected_revision_no).toBe(2)
        await expect(revise).toBeHidden(VISIBLE)
        const after = await details(token, initial)
        expect(after.map((item) => item.current_revision_no)).toEqual([3, 2])
        expect(after.map((item) => item.available_quantity)).toEqual(["0", null])
        expect(after[0]!.input_tax_rate).toMatch(/^0\.130*$/)
    })

    const beforeAvailability = await details(token, initial)
    await selectOfferings(page, prefix, initial.map((item) => item.id))
    await workPage(page).locator("#supplier-offerings-batch-availability").click()
    const availability = workPage(page).getByRole("dialog", { name: "批量更新可供情况", exact: true })
    await expect(availability).toBeVisible(VISIBLE)
    await availability.getByLabel(`${fixture.skus[0]!.sku_no} 可供数量`, { exact: true }).fill("41")
    await availability.getByLabel(`${fixture.skus[1]!.sku_no} 可供数量`, { exact: true }).fill("0")
    await chooseOption(page, availability.getByLabel(`${fixture.skus[1]!.sku_no} 可供状态`, { exact: true }), "不可供")

    await test.step("真实后台任务写入后丢失提交响应；刷新后用原操作号恢复同一任务", async () => {
        let original: TransferBody | undefined
        let jobId: string | undefined
        let committed: BatchResult | undefined
        const matcher = `**${TRANSFERS}`
        await page.route(matcher, async (route) => {
            if (route.request().method() !== "POST" || original) {
                await route.fallback()
                return
            }
            const body = route.request().postDataJSON() as TransferBody
            if (body.kind !== "supply_batch" || body.input.mode !== "availability") {
                await route.fallback()
                return
            }
            original = body
            expect(original.input.validate_only).toBe(false)
            // 等当前 shard 的 Worker 写入，再丢弃浏览器收到的任务登记回执。
            const job = await readUiData<TransferJob>(await route.fetch({ url: `${API_BASE}${TRANSFERS}` }))
            jobId = job.id
            const jobPath = `${TRANSFERS}/${jobId}`
            await expect.poll(async () => (await apiGet<TransferJob>(token, jobPath)).result_file_asset_id, {
                timeout: 30_000,
                message: "后台任务应生成真实写入结果",
            }).toBeTruthy()
            const file = await apiGet<TransferFile>(token, `${jobPath}/result`)
            committed = JSON.parse(Buffer.from(file.content_base64, "base64").toString("utf8")) as BatchResult
            expectStatuses(committed, body.input.rows, ["SUCCEEDED", "SUCCEEDED"])
            await route.abort("failed")
        })
        await availability.locator("#batch-supply-submit").click()
        await expect(availability.getByText(/连接中断，部分行可能已经保存/)).toBeVisible({ timeout: 45_000 })
        expect(original?.request_id).toBeTruthy()
        expect(committed).toBeTruthy()
        for (const sku of fixture.skus) {
            await expect(availability.getByLabel(`${sku.sku_no} 可供数量`, { exact: true })).toBeDisabled()
        }
        const afterCommit = await details(token, initial)
        expect(afterCommit.map((item) => item.available_quantity)).toEqual(["41", "0"])
        expect(afterCommit[1]!.availability_status).toBe("UNAVAILABLE")
        await page.unroute(matcher)
        await selectOfferings(page, prefix, initial.map((item) => item.id))
        await workPage(page).locator("#supplier-offerings-batch-availability").click()
        await expect(availability.getByRole("status")).toContainText("已恢复上次提交记录", VISIBLE)
        for (const [index, sku] of fixture.skus.entries()) {
            const quantity = availability.getByLabel(`${sku.sku_no} 可供数量`, { exact: true })
            await expect(quantity).toHaveValue(index === 0 ? "41" : "0")
            await expect(quantity).toBeDisabled()
        }
        const recovered = await clickSupplyTransfer(page, availability, "availability", "#batch-supply-recover")
        expect(recovered.body).toEqual(original)
        expect(recovered.job.id).toBe(jobId)
        expect(recovered.result).toEqual(committed)
        await expect(availability).toBeHidden(VISIBLE)
        expect(await details(token, initial)).toEqual(afterCommit)
        for (let index = 0; index < afterCommit.length; index += 1) {
            expect(afterCommit[index]!.availability_version).toBe(beforeAvailability[index]!.availability_version + 1)
            expect(afterCommit[index]!.current_revision_id).toBe(beforeAvailability[index]!.current_revision_id)
            expect((await history(token, afterCommit[index]!.id)).items).toHaveLength(beforeAvailability[index]!.current_revision_no)
        }
        const pendingKeys = await page.evaluate(() => Object.keys(sessionStorage).filter((key) => key.startsWith("supply-batch-v1:")))
        expect(pendingKeys).toEqual([])
        for (const item of initial) await expect(workPage(page).locator(`tr[data-row-id="${item.id}"]`)).toContainText(item.supplier_sku_code)
    })
})
