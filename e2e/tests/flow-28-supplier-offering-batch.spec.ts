/**
 * [flow-28] 供应商供给批量新增、调价、可供更新与中断恢复。
 * 合同：docs/supplier-offering-batch-contract.md。
 * 真实 API 验证预检零写入和命令回放；页面验证导入、覆盖确认、冲突重读与会话恢复。
 * 响应丢失场景先调用本次隔离 API 完成写入，再丢弃响应；业务结果不使用 mock。
 */
import fs from "node:fs/promises"

import { test, expect, type Locator, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { openLoggedInWorkspace } from "../helpers/login"
import { readImportWorkbook, writeImportWorkbook } from "../helpers/import-workbook"
import { chooseOption, expectToast } from "../helpers/ui"

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
    supply_region: string[]
    availability_status: string
    availability_version: number
    available_quantity: string | null
}
type Terms = {
    dropship_supply_price_gross: string
    bulk_supply_price_gross: string
    input_tax_rate: string
    bulk_minimum_order_quantity: string
    supply_region: string[]
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
type BatchBody = { rows: BatchRow[]; validate_only: boolean }
type Envelope<T> = { success?: boolean; errorMessage?: string; data: T }
const VISIBLE = { timeout: 20_000 }
const IMPORT_HEADERS = ["公司 SKU 编号", "供应商订货编码", "代发含税价", "集采含税价", "税率 %", "集采起订量", "可供区域", "生效日期", "可供状态", "可供数量"]

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
        supply_region: ["全国"],
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

async function batch(token: string, mode: "create" | "revise" | "availability", rows: BatchRow[], validateOnly = false) {
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
    const otherSupplier = suppliers.items.find((item) => item.supplier_no === "SUP-DEV-WEEK")
    const owner = admins.find((item) => item.account === "caigou")
    expect(category && brand && unit && supplier && otherSupplier && owner, "批量验收需要固定目录、有效实物供应商及采购岗位种子").toBeTruthy()
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
    return { skus, supplier: supplier!, otherSupplier: otherSupplier! }
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

test("批量API：整批预检阻断、容器限制、三类命令回放与版本冲突", async () => {
    const prefix = `E2E-BATCH-API-${Date.now().toString(36).toUpperCase()}`
    const setupToken = await apiToken("admin")
    const token = await apiToken("caigou")
    const fixture = await prepareSkus(setupToken, prefix)
    const createRows: BatchRow[] = fixture.skus.map((sku, index) => ({
        row_id: sku.id,
        input: {
            sku_id: sku.id,
            supplier_id: fixture.supplier.id,
            supplier_sku_code: `000${prefix}-${index}`,
            source_type: "MANUAL",
            terms: terms(),
            availability_status: "AVAILABLE",
            available_quantity: index === 0 ? "0" : null,
            change_reason: "E2E 批量登记",
            idempotency_key: `${prefix}-create-${index}`,
        },
    }))

    await test.step("一个非法行阻止其他有效行写入；校验模式不写入", async () => {
        const invalid = structuredClone(createRows)
        invalid[1]!.input.terms = { ...terms(), dropship_supply_price_gross: "-1.00" }
        for (const validateOnly of [true, false]) {
            const result = await batch(token, "create", invalid, validateOnly)
            expectStatuses(result, invalid, ["READY", "INVALID"])
            expect(result.rows[1]!.message).toBeTruthy()
            expect(await offerings(token, prefix)).toEqual([])
        }
        expectStatuses(await batch(token, "create", createRows, true), createRows, ["READY", "READY"])
        expect(await offerings(token, prefix)).toEqual([])
    })

    await test.step("空批、超限、重复行、命令、身份和混合供应商拒绝整个容器", async () => {
        const cases: Array<{ rows: BatchRow[]; message: RegExp }> = [
            { rows: [], message: /1–100/ },
            { rows: Array.from({ length: 101 }, () => createRows[0]!), message: /1–100/ },
            { rows: [{ ...createRows[0]! }, { ...createRows[1]!, row_id: createRows[0]!.row_id }], message: /行标识/ },
            { rows: [createRows[0]!, { ...createRows[1]!, input: { ...createRows[1]!.input, idempotency_key: createRows[0]!.input.idempotency_key } }], message: /提交标识/ },
            { rows: [createRows[0]!, { ...createRows[1]!, input: { ...createRows[1]!.input, supplier_sku_code: createRows[0]!.input.supplier_sku_code } }], message: /重复的供给/ },
            { rows: [createRows[0]!, { ...createRows[1]!, input: { ...createRows[1]!.input, supplier_id: fixture.otherSupplier.id } }], message: /一个供应商/ },
        ]
        for (const item of cases) {
            const result = await postRaw<BatchResult>(token, "/admin/supplier-offerings/batch/create", { rows: item.rows, validate_only: false })
            expect(result.envelope.success).toBe(false)
            expect(result.envelope.errorMessage).toMatch(item.message)
        }
        expect(await offerings(token, prefix)).toEqual([])
    })

    const created = await batch(token, "create", createRows)
    expectStatuses(created, createRows, ["SUCCEEDED", "SUCCEEDED"])
    const initial = await offerings(token, prefix)
    expect(initial).toHaveLength(2)
    expect(initial.map((row) => row.available_quantity)).toEqual(["0", null])
    expect(initial.map((row) => row.current_revision_no)).toEqual([1, 1])
    await test.step("新增原命令恢复成功回执，不追加条款或可供版本；异载荷键冲突", async () => {
        for (const validateOnly of [true, false]) {
            expect(await batch(token, "create", createRows, validateOnly)).toEqual(created)
            expect(await details(token, initial)).toEqual(initial)
        }
        const changed = structuredClone(createRows)
        changed[0]!.input.available_quantity = "7"
        expectStatuses(await batch(token, "create", changed), changed, ["INVALID", "SUCCEEDED"])
        expect(await details(token, initial)).toEqual(initial)
    })

    const reviseRows: BatchRow[] = initial.map((offering, index) => ({
        row_id: offering.id,
        input: { offering_id: offering.id, command: {
            expected_revision_no: offering.current_revision_no,
            terms: terms("26.00", "24.00"),
            change_reason: "E2E 批量调价",
            idempotency_key: `${prefix}-revise-${index}`,
        } },
    }))
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
        expect(await batch(token, "revise", reviseRows)).toEqual(revised)
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

async function readUiResult(response: Response): Promise<BatchResult> {
    const envelope = await response.json() as Envelope<BatchResult>
    expect(response.ok() && envelope.success !== false, envelope.errorMessage ?? response.statusText()).toBe(true)
    return envelope.data
}

async function clickBatch(page: Page, dialog: Locator, mode: string, control: string): Promise<{ body: BatchBody; result: BatchResult }> {
    const [response] = await Promise.all([
        page.waitForResponse((item) => item.request().method() === "POST" && new URL(item.url()).pathname === `/admin/supplier-offerings/batch/${mode}`),
        dialog.locator(control).click(),
    ])
    return { body: response.request().postDataJSON() as BatchBody, result: await readUiResult(response) }
}

async function selectOfferings(page: Page, prefix: string, ids: string[]) {
    await page.goto(`/procurement/supplier-offerings?q=${encodeURIComponent(prefix)}`)
    for (const id of ids) {
        const row = page.locator(`tr[data-row-id="${id}"]`)
        await expect(row).toBeVisible(VISIBLE)
        await row.getByRole("checkbox").check()
    }
    await expect(page.getByText(`本页已选 ${ids.length} 行`, { exact: true })).toBeVisible()
}

function csv(rows: readonly (readonly string[])[]): Buffer {
    return Buffer.from([IMPORT_HEADERS, ...rows].map((row) => row.map((cell) => `"${cell.replaceAll('"', '""')}"`).join(",")).join("\r\n"), "utf8")
}

test("批量页面：文件精确匹配、公共设置、冲突重读及真实提交响应丢失后恢复", async ({ browser }, testInfo) => {
    const prefix = `E2E-BATCH-UI-${Date.now().toString(36).toUpperCase()}`
    const fixture = await prepareSkus(await apiToken("admin"), prefix)
    const { page } = await openLoggedInWorkspace(browser, "caigou")
    const token = await apiToken("caigou")
    await page.goto(`/procurement/supplier-offerings?q=${encodeURIComponent(prefix)}&supplierId=${fixture.supplier.id}`)
    await page.locator("#supplier-offerings-page-batch-create").click()
    const create = page.getByRole("dialog", { name: "批量添加供给", exact: true })
    await expect(create).toBeVisible(VISIBLE)
    await expect(create.locator("#batch-supply-supplier")).not.toHaveValue("")
    const sourceRows = fixture.skus.map((sku, index) => [sku.sku_no, `000${prefix}-${index}`, index === 0 ? "17.00" : "19.00", "15.00", "9", "5", "全国", businessDate(), "可供", index === 0 ? "0" : ""])

    await test.step("下载真实模板；未知和重复 SKU 文件整次拒绝，正确导入仅添加待提交行", async () => {
        const [download] = await Promise.all([page.waitForEvent("download"), create.locator("#batch-supply-template").click()])
        expect(download.suggestedFilename()).toMatch(/\.xlsx$/)
        const output = testInfo.outputPath("supply-template.xlsx")
        await download.saveAs(output)
        expect((await fs.readFile(output)).subarray(0, 2).toString()).toBe("PK")
        const template = (await readImportWorkbook(output)).getWorksheet("供给配置")!
        expect(template.getCell("A1").text).toBe("公司 SKU 编号")
        expect(template.getCell("B1").text).toBe("供应商订货编码")
        expect(template.getColumn(2).numFmt).toBe("@")
        for (const [rows, message] of [
            [[sourceRows[0]!, [`${prefix}-MISSING`, ...sourceRows[1]!.slice(1)]], /不存在、无权选择或匹配不唯一/],
            [[sourceRows[0]!, sourceRows[0]!], /文件中重复/],
            [[sourceRows[0]!, [...sourceRows[1]!.slice(0, 8), "不可识别状态", sourceRows[1]![9]!]], /可供状态无法识别/],
        ] as const) {
            await create.locator("#batch-supply-file").setInputFiles({ name: "invalid.csv", mimeType: "text/csv", buffer: csv([...rows]) })
            await expect(create.getByRole("status")).toContainText(message, VISIBLE)
            await expect(create.getByRole("status")).toContainText(prefix)
            await expect(create.getByRole("checkbox", { name: /^勾选 E2E/ })).toHaveCount(0)
            expect(await offerings(token, prefix)).toEqual([])
        }

        const formulaFile = await writeImportWorkbook(testInfo.outputPath("supply-formula.xlsx"), "供给配置", IMPORT_HEADERS, [
            sourceRows[0]!,
            [sourceRows[1]![0]!, { formula: 'CONCAT("000","123")', result: "000123" }, ...sourceRows[1]!.slice(2)],
        ])
        await create.locator("#batch-supply-file").setInputFiles(formulaFile)
        await expect(create.getByRole("status")).toContainText("第 3 行包含公式", VISIBLE)
        const rejectedFiles = [
            { name: "too-large.csv", mimeType: "text/csv", buffer: Buffer.alloc(5 * 1024 * 1024 + 1, "x"), message: "文件不能超过 5 MB" },
            { name: "too-many.csv", mimeType: "text/csv", buffer: csv(Array.from({ length: 101 }, () => sourceRows[0]!)), message: "每批最多 100 行" },
            { name: "unsupported.txt", mimeType: "text/plain", buffer: csv(sourceRows), message: "请选择 .xlsx 或 UTF-8 CSV 文件" },
        ]
        for (const file of rejectedFiles) {
            await create.locator("#batch-supply-file").setInputFiles({ name: file.name, mimeType: file.mimeType, buffer: file.buffer })
            await expect(create.getByRole("status")).toContainText(file.message, VISIBLE)
            await expect(create.getByRole("checkbox", { name: /^勾选 E2E/ })).toHaveCount(0)
        }
        expect(await offerings(token, prefix)).toEqual([])

        // 两种文件格式分别通过生产解析器加入一行，订货编号全程保持文本。
        await create.locator("#batch-supply-file").setInputFiles({ name: "supply.csv", mimeType: "text/csv", buffer: csv([sourceRows[0]!]) })
        await expect(create.getByLabel(`${fixture.skus[0]!.sku_no} 供应商订货编码`, { exact: true })).toHaveValue(sourceRows[0]![1]!, VISIBLE)
        const workbookFile = await writeImportWorkbook(testInfo.outputPath("supply.xlsx"), "供给配置", IMPORT_HEADERS, [sourceRows[1]!])
        await create.locator("#batch-supply-file").setInputFiles(workbookFile)
        for (const [index, sku] of fixture.skus.entries()) {
            await expect(create.getByLabel(`${sku.sku_no} 供应商订货编码`, { exact: true })).toHaveValue(sourceRows[index]![1]!, VISIBLE)
        }
        expect(await offerings(token, prefix)).toEqual([])
    })

    await test.step("窄窗口内标题和提交区保持可见，宽表只在表格容器滚动", async () => {
        const desktop = page.viewportSize() ?? { width: 1440, height: 900 }
        await page.setViewportSize({ width: 390, height: 844 })
        await expect(create.getByRole("heading", { name: "批量添加供给", exact: true })).toBeInViewport()
        await expect(create.locator("#batch-supply-submit")).toBeInViewport()
        await expect.poll(async () => {
            const bounds = await create.boundingBox()
            return bounds != null && bounds.x >= 0 && bounds.x + bounds.width <= 391
        }).toBe(true)
        const tableOverflow = await create.locator('[data-slot="table-container"]').evaluate((node) => ({
            width: node.clientWidth,
            contentWidth: node.scrollWidth,
            overflowX: getComputedStyle(node).overflowX,
        }))
        expect(tableOverflow.contentWidth).toBeGreaterThan(tableOverflow.width)
        expect(tableOverflow.overflowX).toBe("auto")
        const dialogOverflow = await create.evaluate((node) => node.scrollWidth - node.clientWidth)
        expect(dialogOverflow).toBeLessThanOrEqual(1)
        await page.setViewportSize(desktop)
    })

    await test.step("TSV多行粘贴保留文本订货编码，超出表格范围整次拒绝", async () => {
        const first = create.getByLabel(`${fixture.skus[0]!.sku_no} 供应商订货编码`, { exact: true })
        const pasteRows = sourceRows.map((row) => row.slice(1, 7).map((cell) => `"${cell.replaceAll('"', '""')}"`).join("\t"))
        await first.evaluate((input, text) => {
            const clipboard = new DataTransfer()
            clipboard.setData("text/plain", text)
            input.dispatchEvent(new ClipboardEvent("paste", { bubbles: true, cancelable: true, clipboardData: clipboard }))
        }, pasteRows.join("\n"))
        await expect(create.getByRole("status")).toHaveText("已粘贴 2 行，请核对后校验。")
        for (const [index, sku] of fixture.skus.entries()) {
            await expect(create.getByLabel(`${sku.sku_no} 供应商订货编码`, { exact: true })).toHaveValue(sourceRows[index]![1]!)
        }
        await first.evaluate((input) => {
            const clipboard = new DataTransfer()
            clipboard.setData("text/plain", "unexpected-A\nunexpected-B\nunexpected-C")
            input.dispatchEvent(new ClipboardEvent("paste", { bubbles: true, cancelable: true, clipboardData: clipboard }))
        })
        await expect(create.getByRole("status")).toContainText("粘贴范围超出表格")
        for (const [index, sku] of fixture.skus.entries()) {
            await expect(create.getByLabel(`${sku.sku_no} 供应商订货编码`, { exact: true })).toHaveValue(sourceRows[index]![1]!)
            await expect(create.getByLabel(`${sku.sku_no} 代发含税价`, { exact: true })).toHaveValue(sourceRows[index]![2]!)
        }
        expect(await offerings(token, prefix)).toEqual([])
    })

    await test.step("公共补齐保留已有值，覆盖必须确认；提交前预检无写入", async () => {
        await create.locator("#batch-supply-common-dropshipprice").fill("22.00")
        await create.locator("#batch-supply-common-bulkprice").fill("20.00")
        await create.getByLabel(`${fixture.skus[1]!.sku_no} 集采含税价`, { exact: true }).fill("")
        await create.locator("#batch-supply-fill-empty").click()
        await expect(create.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })).toHaveValue("17.00")
        await expect(create.getByLabel(`${fixture.skus[0]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("15.00")
        await expect(create.getByLabel(`${fixture.skus[1]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("20.00")
        await create.locator("#batch-supply-overwrite").click()
        await expect(create.locator("#batch-supply-overwrite-confirm")).toBeVisible()
        await expect(create.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })).toHaveValue("17.00")
        await create.locator("#batch-supply-overwrite-confirm").click()
        for (const sku of fixture.skus) {
            await expect(create.getByLabel(`${sku.sku_no} 代发含税价`, { exact: true })).toHaveValue("22.00")
            await expect(create.getByLabel(`${sku.sku_no} 集采含税价`, { exact: true })).toHaveValue("20.00")
            await expect(create.getByLabel(`${sku.sku_no} 可供区域`, { exact: true })).toHaveValue("全国")
        }
        await expect(create.locator("#batch-supply-submit")).toHaveText("校验并提交 2 行")
        const validated = await clickBatch(page, create, "create", "#batch-supply-validate")
        expect(validated.body.validate_only).toBe(true)
        expectStatuses(validated.result, validated.body.rows, ["READY", "READY"])
        expect(await offerings(token, prefix)).toEqual([])
        const submitted = await clickBatch(page, create, "create", "#batch-supply-submit")
        expectStatuses(submitted.result, submitted.body.rows, ["SUCCEEDED", "SUCCEEDED"])
        expect(submitted.body.rows.map((row) => row.input.source_type)).toEqual(["EXCEL", "EXCEL"])
        await expectToast(page, "批量添加供给完成")
        await expect(create).toBeHidden(VISIBLE)
    })

    const initial = await offerings(token, prefix)
    expect(initial).toHaveLength(2)
    expect(initial.map((item) => item.supplier_sku_code)).toEqual(sourceRows.map((row) => row[1]))
    expect(initial.map((item) => item.available_quantity)).toEqual(["0", null])
    await selectOfferings(page, prefix, initial.map((item) => item.id))
    await page.locator("#supplier-offerings-batch-revise").click()
    const revise = page.getByRole("dialog", { name: "批量调价与条款", exact: true })
    await expect(revise).toBeVisible(VISIBLE)

    await test.step("外部修改导致版本冲突；重新读取替换完整最新条款后再次修改提交", async () => {
        await revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true }).fill("99.00")
        const externalTerms = { ...terms("34.00", "33.00"), supply_region: ["华东"] }
        await post(token, `/admin/supplier-offerings/${initial[0]!.id}/revisions`, {
            expected_revision_no: initial[0]!.current_revision_no,
            terms: externalTerms,
            change_reason: "E2E 模拟其他会话修改最新条款",
            idempotency_key: `${prefix}-external-revise`,
        })
        const conflict = await clickBatch(page, revise, "revise", "#batch-supply-submit")
        expect(conflict.result.rows.find((row) => row.row_id === initial[0]!.id)?.status).toBe("INVALID")
        expect(conflict.result.rows.find((row) => row.row_id === initial[1]!.id)?.status).toBe("READY")
        const blocked = await details(token, initial)
        expect(blocked.map((item) => item.current_revision_no)).toEqual([2, 1])
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })).toHaveValue("99.00")
        await revise.locator("#batch-supply-reload-failed").click()
        await expect(revise.locator("#batch-supply-reload-confirm")).toBeVisible()
        await revise.locator("#batch-supply-reload-confirm").click()
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 代发含税价`, { exact: true })).toHaveValue("34.00", VISIBLE)
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 集采含税价`, { exact: true })).toHaveValue("33.00")
        await expect(revise.getByLabel(`${fixture.skus[0]!.sku_no} 可供区域`, { exact: true })).toHaveValue("华东")
        for (const [index, sku] of fixture.skus.entries()) {
            await revise.getByLabel(`${sku.sku_no} 代发含税价`, { exact: true }).fill(index === 0 ? "35.00" : "36.00")
        }
        const saved = await clickBatch(page, revise, "revise", "#batch-supply-submit")
        expectStatuses(saved.result, saved.body.rows, ["SUCCEEDED", "SUCCEEDED"])
        expect((saved.body.rows.find((row) => row.row_id === initial[0]!.id)!.input.command as Record<string, unknown>).expected_revision_no).toBe(2)
        await expect(revise).toBeHidden(VISIBLE)
        const after = await details(token, initial)
        expect(after.map((item) => item.current_revision_no)).toEqual([3, 2])
        expect(after.map((item) => item.available_quantity)).toEqual(["0", null])
        expect(after[0]!.supply_region).toEqual(["华东"])
    })

    const beforeAvailability = await details(token, initial)
    await selectOfferings(page, prefix, initial.map((item) => item.id))
    await page.locator("#supplier-offerings-batch-availability").click()
    const availability = page.getByRole("dialog", { name: "批量更新可供情况", exact: true })
    await expect(availability).toBeVisible(VISIBLE)
    await availability.getByLabel(`${fixture.skus[0]!.sku_no} 可供数量`, { exact: true }).fill("41")
    await availability.getByLabel(`${fixture.skus[1]!.sku_no} 可供数量`, { exact: true }).fill("0")
    await chooseOption(page, availability.getByLabel(`${fixture.skus[1]!.sku_no} 可供状态`, { exact: true }), "不可供")

    await test.step("真实服务写入后丢失响应，锁定原命令；页面刷新恢复会话并只重放原内容", async () => {
        let original: BatchBody | undefined
        let committed: BatchResult | undefined
        const matcher = "**/admin/supplier-offerings/batch/availability"
        await page.route(matcher, async (route) => {
            const body = route.request().postDataJSON() as BatchBody
            if (route.request().method() !== "POST" || body.validate_only || original) {
                await route.fallback()
                return
            }
            original = body
            // 明确使用当前 shard；先取得真实成功回执，再模拟浏览器收不到回执。
            const response = await route.fetch({ url: `${API_BASE}/admin/supplier-offerings/batch/availability` })
            const envelope = await response.json() as Envelope<BatchResult>
            expect(response.ok() && envelope.success !== false, envelope.errorMessage).toBe(true)
            committed = envelope.data
            expectStatuses(committed, body.rows, ["SUCCEEDED", "SUCCEEDED"])
            await route.abort("failed")
        })
        await availability.locator("#batch-supply-submit").click()
        await expect(availability.getByText(/连接中断，部分行可能已经保存/)).toBeVisible(VISIBLE)
        expect(original).toBeTruthy()
        expect(committed).toBeTruthy()
        for (const sku of fixture.skus) {
            await expect(availability.getByLabel(`${sku.sku_no} 可供数量`, { exact: true })).toBeDisabled()
        }
        const afterCommit = await details(token, initial)
        expect(afterCommit.map((item) => item.available_quantity)).toEqual(["41", "0"])
        expect(afterCommit[1]!.availability_status).toBe("UNAVAILABLE")
        await page.unroute(matcher)
        await selectOfferings(page, prefix, initial.map((item) => item.id))
        await page.locator("#supplier-offerings-batch-availability").click()
        await expect(availability.getByRole("status")).toContainText("已恢复上次提交记录", VISIBLE)
        for (const [index, sku] of fixture.skus.entries()) {
            const quantity = availability.getByLabel(`${sku.sku_no} 可供数量`, { exact: true })
            await expect(quantity).toHaveValue(index === 0 ? "41" : "0")
            await expect(quantity).toBeDisabled()
        }
        const recovered = await clickBatch(page, availability, "availability", "#batch-supply-recover")
        expect(recovered.body).toEqual(original)
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
        for (const item of initial) await expect(page.locator(`tr[data-row-id="${item.id}"]`)).toContainText(item.supplier_sku_code)
    })
})
