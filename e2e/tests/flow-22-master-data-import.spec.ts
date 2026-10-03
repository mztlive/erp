/**
 * [flow-22] 真实 XLSX 导入与后台任务结果。
 * 商品走浏览器分片直传、服务端解析和逐行建档；供应商走页面工作簿解析、加密源数据和逐行建档。
 * 验证同 SPU 多 SKU、公司参考价与供给成本边界、重复跳过、失败行保留及失败工作簿下载。
 * 全部写入由页面办理；API 仅核对生产结果和同一提交的幂等回放。
 */
import { test, expect, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { newLoggedInContext } from "../helpers/login"
import {
    writeImportWorkbook,
    readImportWorkbook,
} from "../helpers/import-workbook"

const VISIBLE = { timeout: 20_000 }
const PRODUCT_HEADERS = [
    "产品编码",
    "产品条码",
    "产品主图",
    "产品副图",
    "产品副图",
    "品牌",
    "产品类别",
    "产品类别编码",
    "产品名称",
    "产品规格",
    "一件代发成本价（含税运）",
    "集采成本价（含税）",
    "一件代发底价（含税运）",
    "集采底价（含税）",
    "集采起订量",
    "一件代发快递",
    "市场价",
    "供应商名称",
    "供应商编号",
    "食品产品有效期",
    "生产批次",
    "是否自营产品",
    "自营产品库存",
    "商品税率",
    "出厂价（含税）",
]
const SUPPLIER_HEADERS = [
    "供应商编号",
    "供应商全称",
    "联系人",
    "联系方式",
    "对公银行账号",
    "开户行",
    "供应商地址",
    "公司签约主体",
    "公司付款主体",
    "结算方式",
    "经营类目",
    "合同编号",
    "合同有效期",
    "合同文件",
    "授权书文件",
    "授权书有效期",
    "食品经营许可证",
    "供应商法人身份证",
    "发票类型",
    "发票税点",
    "供应商合作期初评分",
    "供应商评级",
    "供应商合作中评分",
]

type ApiPage<T> = { items: T[]; total: number }
type Job = {
    id: string
    job_no: string
    requested_by: string
    domain_job_type: string
    status: string
    total_count: number
    processed_count: number
    success_count: number
    skipped_count: number
    failed_count: number
    finished_at: number | null
}
type JobItem = {
    source_row_no: number
    status: string
    result_code: string | null
    result_summary: string | null
    result_object_id: string | null
}
type Sku = {
    id: string
    sku_no: string
    current_revision_id: string
    specification_signature: string
}
type SkuRevision = {
    id: string
    sku_id: string
    factory_price_gross: string | null
    sales_visible_price_gross: string | null
    bulk_price_gross: string | null
    bulk_min_quantity: string | null
    market_price: string | null
}

async function responseData<T>(response: Response): Promise<T> {
    const body = await response.json()
    expect(
        response.ok(),
        `HTTP ${response.status()}: ${JSON.stringify(body)}`,
    ).toBeTruthy()
    expect(body.success).not.toBe(false)
    expect(body.data).toBeTruthy()
    return body.data as T
}

async function finishedJob(token: string, jobId: string): Promise<Job> {
    let latest: Job | undefined
    await expect
        .poll(
            async () => {
                latest = await apiGet<Job>(
                    token,
                    `/admin/background-jobs/${jobId}`,
                )
                return latest.finished_at !== null
            },
            {
                timeout: 120_000,
                intervals: [500, 1_000, 2_000],
                message: "逐行导入任务必须到达有结束时间的终态",
            },
        )
        .toBe(true)
    expect(latest!.processed_count).toBe(latest!.total_count)
    return latest!
}

async function openJobResults(page: Page, job: Job, expectedRows: number) {
    await page.goto("/governance/background-jobs")
    await page
        .locator("#governance-background-jobs-toolbar-search-input")
        .fill(job.job_no)
    await page.locator("#governance-background-jobs-toolbar-query").click()
    const row = page
        .locator("#governance-background-jobs-table tbody tr")
        .filter({ hasText: job.job_no })
    await expect(row).toHaveCount(1, VISIBLE)
    await row.click()
    const sheet = page.getByRole("dialog")
    await expect(sheet.getByRole("heading", { name: "逐项结果" })).toBeVisible(
        VISIBLE,
    )
    await expect(sheet.locator("li")).toHaveCount(expectedRows, VISIBLE)
    await expect(
        sheet
            .locator('[data-slot="quick-preview-summary"]')
            .getByText("部分成功", { exact: true }),
    ).toBeVisible(VISIBLE)
    await expect(
        page.locator("#governance-background-jobs-preview-cancel"),
    ).toHaveCount(0)
    return sheet
}

async function importProducts(page: Page, filePath: string): Promise<Job> {
    await page.goto("/master-data/products")
    await page.locator("#master-data-products-list-import").click()
    await page.locator("#product-import-file").setInputFiles(filePath)
    const completed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            /\/admin\/products\/import-uploads\/[^/]+\/complete$/.test(
                new URL(response.url()).pathname,
            ),
        { timeout: 90_000 },
    )
    await page.locator("#product-import-submit").click()
    const job = await responseData<Job>(await completed)
    await expect(page.locator("#product-import-dialog")).toHaveCount(0, VISIBLE)
    return job
}

function productRow(input: {
    productNo: string
    name: string
    specification: string
    category: string
    brand: string
    suffix: string
}) {
    const row = Array<string>(PRODUCT_HEADERS.length).fill("")
    for (const [index, value] of [
        [0, input.productNo],
        [1, `00${input.suffix}${input.specification === "单盒" ? "1" : "2"}`],
        [5, input.brand],
        [6, input.category],
        [8, input.name],
        [9, input.specification],
        [10, "12.00"],
        [11, "11.00"],
        [12, "49.00"],
        [13, "45.00"],
        [14, "10"],
        [16, "59.00"],
        [17, "导入文件内的供应商不会自动建供给"],
        [18, "0000123"],
        [24, "31.00"],
    ] as const)
        row[index] = value
    return row
}

test("[flow-22] 商品报价表分片直传：同 SPU 多 SKU、失败行与重复重导", async ({
    browser,
}, testInfo) => {
    test.setTimeout(240_000)
    const { page } = await newLoggedInContext(browser, "admin")
    const token = await apiToken("admin")
    const suffix = `${Date.now()}${testInfo.workerIndex}`
    const productNo = `E2E-IMPORT-${suffix}`
    const productName = `E2E导入礼盒${suffix}`
    const [categories, brands] = await Promise.all([
        apiGet<ApiPage<{ name: string; category_code: string }>>(
            token,
            "/admin/product-categories",
            {
                page: 1,
                page_size: 100,
            },
        ),
        apiGet<ApiPage<{ name: string; brand_code: string }>>(
            token,
            "/admin/product-brands",
            {
                page: 1,
                page_size: 100,
            },
        ),
    ])
    const category = categories.items.find(
        (item) => item.category_code === "TEA",
    )
    const brand = brands.items.find((item) => item.brand_code === "SF")
    expect(category, "固定种子须包含茶叶分类").toBeTruthy()
    expect(brand, "固定种子须包含狮峰品牌").toBeTruthy()
    const firstRow = productRow({
        productNo,
        name: productName,
        specification: "单盒",
        category: category!.name,
        brand: brand!.name,
        suffix,
    })
    const secondRow = productRow({
        productNo,
        name: `${productName}双盒`,
        specification: "双盒",
        category: category!.name,
        brand: brand!.name,
        suffix,
    })
    const badRow = productRow({
        productNo: `${productNo}-BAD`,
        name: `E2E失败商品${suffix}`,
        specification: "单盒",
        category: "",
        brand: brand!.name,
        suffix,
    })
    const file = await writeImportWorkbook(
        testInfo.outputPath("商品导入.xlsx"),
        "对内",
        PRODUCT_HEADERS,
        [firstRow, secondRow, badRow],
    )

    const job = await finishedJob(token, (await importProducts(page, file)).id)
    expect(job).toMatchObject({
        domain_job_type: "PRODUCT_IMPORT",
        status: "partially_succeeded",
        total_count: 3,
        success_count: 2,
        skipped_count: 0,
        failed_count: 1,
    })
    const items = await apiGet<ApiPage<JobItem>>(
        token,
        `/admin/background-jobs/${job.id}/items`,
        {
            page: 1,
            page_size: 100,
        },
    )
    expect(
        items.items.map((item) => [item.source_row_no, item.status]),
    ).toEqual([
        [2, "success"],
        [3, "success"],
        [4, "failed"],
    ])
    expect(items.items[2].result_summary).toContain("产品类别不能为空")
    const sheet = await openJobResults(page, job, 3)
    await expect(
        sheet.getByText("Excel 第 4 行", { exact: true }),
    ).toBeVisible()
    await expect(
        sheet.getByText("产品类别不能为空", { exact: false }),
    ).toBeVisible()

    const products = await apiGet<ApiPage<{ id: string; product_no: string }>>(
        token,
        "/admin/products",
        { keyword: productNo, page: 1, page_size: 100 },
    )
    expect(products.items).toHaveLength(1)
    const productId = products.items[0].id
    const skus = await apiGet<ApiPage<Sku>>(
        token,
        `/admin/products/${productId}/skus`,
        {
            page: 1,
            page_size: 100,
        },
    )
    expect(skus.items.map((sku) => sku.sku_no).sort()).toEqual([
        `${productNo}-01`,
        `${productNo}-02`,
    ])
    const revisions = await apiGet<ApiPage<SkuRevision>>(
        token,
        `/admin/products/${productId}/sku-revisions`,
        { page: 1, page_size: 100 },
    )
    const currentRevisions = skus.items.map((sku) =>
        revisions.items.find(
            (revision) => revision.id === sku.current_revision_id,
        ),
    )
    expect(skus.items.map((sku) => sku.specification_signature).sort()).toEqual(
        ["规格=单盒", "规格=双盒"],
    )
    for (const revision of currentRevisions) {
        expect(revision).toMatchObject({
            factory_price_gross: "31.00",
            sales_visible_price_gross: "49.00",
            bulk_price_gross: "45.00",
            bulk_min_quantity: "10",
            market_price: "59.00",
        })
    }
    const offerings = await apiGet<ApiPage<unknown>>(
        token,
        "/admin/supplier-offerings",
        {
            product_no: productNo,
            page: 1,
            page_size: 100,
        },
    )
    expect(
        offerings.total,
        "商品导入不得以供应商名称、编号或成本价隐式创建供给",
    ).toBe(0)
    await page.goto(`/master-data/products/${productId}`)
    await expect(
        page.getByRole("heading", { name: productName, exact: true }),
    ).toBeVisible(VISIBLE)
    await expect(
        page.getByText(`${productNo}-01`, { exact: false }),
    ).toBeVisible(VISIBLE)
    await expect(
        page.getByText(`${productNo}-02`, { exact: false }),
    ).toBeVisible(VISIBLE)

    const replay = await finishedJob(
        token,
        (await importProducts(page, file)).id,
    )
    expect(replay.id).not.toBe(job.id)
    expect(replay).toMatchObject({
        total_count: 3,
        success_count: 0,
        skipped_count: 2,
        failed_count: 1,
    })
    const after = await apiGet<ApiPage<Sku>>(
        token,
        `/admin/products/${productId}/skus`,
        {
            page: 1,
            page_size: 100,
        },
    )
    expect(
        after.items.map((sku) => [sku.id, sku.current_revision_id]).sort(),
    ).toEqual(skus.items.map((sku) => [sku.id, sku.current_revision_id]).sort())
})

test("[flow-22] 供应商模板：逐行创建与跳过、失败下载和原提交人权限", async ({
    browser,
}, testInfo) => {
    test.setTimeout(180_000)
    const { page, context } = await newLoggedInContext(browser, "admin")
    const token = await apiToken("admin")
    const suffix = `${Date.now()}${testInfo.workerIndex}`
    const supplierName = `E2E导入供应商${suffix}有限公司`
    const invalidName = `E2E缺少结算${suffix}有限公司`
    const formulaName = `E2E含公式${suffix}有限公司`
    const companies = await apiGet<
        ApiPage<{ id: string; legal_name: string; status: string }>
    >(token, "/admin/companies", { status: "active", page: 1, page_size: 100 })
    const company = companies.items.find((item) => item.status === "active")
    expect(company, "固定种子必须存在已启用的我方公司主体").toBeTruthy()
    const valid = Array<string>(SUPPLIER_HEADERS.length).fill("")
    for (const [index, value] of [
        [0, "0000123"],
        [1, supplierName],
        [2, "导入联系人"],
        [3, "13800138000"],
        [4, "00123456789012345678"],
        [5, "E2E开户银行"],
        [7, company!.legal_name],
        [9, "月结"],
        [10, "茶叶礼盒"],
        [18, "专票"],
        [19, "9%、13%"],
    ] as const)
        valid[index] = value
    const duplicate = [...valid]
    duplicate[0] = "0000456"
    const invalid = [...valid]
    invalid[1] = invalidName
    invalid[9] = ""
    const formula = [...valid] as Array<
        string | { formula: string; result: number }
    >
    formula[1] = formulaName
    formula[4] = { formula: "1+1", result: 2 }
    const file = await writeImportWorkbook(
        testInfo.outputPath("供应商导入.xlsx"),
        "供应商信息",
        SUPPLIER_HEADERS,
        [valid, duplicate, invalid, formula],
    )
    await page.goto("/master-data/suppliers")
    await page.locator("#master-data-suppliers-list-import").click()
    await page.locator("#supplier-import-file").setInputFiles(file)
    const dialog = page.locator("#supplier-import-dialog")
    await expect(
        dialog.getByText("已读取 4 行，提交后后台逐行导入。", { exact: false }),
    ).toBeVisible(VISIBLE)
    await expect(
        dialog.getByText("导入单元格不能包含公式", { exact: false }),
    ).toBeVisible()
    const submitted = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname ===
                "/admin/supplier-profiles/import/jobs",
    )
    await page.locator("#supplier-import-submit").click()
    const submittedResponse = await submitted
    const requestBody = submittedResponse.request().postDataJSON()
    const job = await finishedJob(
        token,
        (await responseData<Job>(submittedResponse)).id,
    )
    const items = await apiGet<ApiPage<JobItem>>(
        token,
        `/admin/background-jobs/${job.id}/items`,
        {
            page: 1,
            page_size: 100,
        },
    )
    await testInfo.attach("供应商导入逐行结果", {
        body: JSON.stringify({ job, items: items.items }, null, 2),
        contentType: "application/json",
    })
    expect(job, JSON.stringify(items.items)).toMatchObject({
        domain_job_type: "SUPPLIER_IMPORT",
        status: "partially_succeeded",
        total_count: 4,
        success_count: 1,
        skipped_count: 1,
        failed_count: 2,
    })
    expect(
        items.items.map((item) => [item.source_row_no, item.status]),
    ).toEqual([
        [2, "success"],
        [3, "skipped"],
        [4, "failed"],
        [5, "failed"],
    ])
    expect(items.items[0].result_object_id).toBeTruthy()
    expect(items.items[1].result_object_id).toBe(
        items.items[0].result_object_id,
    )
    expect(items.items[2].result_summary).toContain("缺少结算方式")
    expect(items.items[3].result_summary).toContain("无法安全读取")
    const suppliers = await apiGet<ApiPage<{ id: string; legal_name: string }>>(
        token,
        "/admin/suppliers",
        { keyword: suffix, page: 1, page_size: 100 },
    )
    expect(suppliers.items.map((supplier) => supplier.legal_name)).toEqual([
        supplierName,
    ])
    const supplier = await apiGet<{
        maintainer_user_id: string
        current_profile: {
            signing_entity_party_id: string
            payment_entity_party_id: string
            invoice_tax_rates: string[]
            payment_term_snapshot: string
        }
    }>(token, `/admin/suppliers/${suppliers.items[0].id}`)
    expect(
        supplier.maintainer_user_id,
        "导入创建必须由原提交人承担维护责任",
    ).toBe(job.requested_by)
    expect(supplier.current_profile).toMatchObject({
        signing_entity_party_id: company!.id,
        payment_entity_party_id: company!.id,
        invoice_tax_rates: ["0.09", "0.13"],
        payment_term_snapshot: "PERIOD_MONTH_15",
    })
    const sheet = await openJobResults(page, job, 4)
    await expect(
        sheet.getByText(invalidName.slice(0, 30), { exact: true }),
    ).toBeVisible()
    const downloaded = page.waitForEvent("download")
    await page.locator("#supplier-import-download-failures").click()
    const download = await downloaded
    expect(download.suggestedFilename()).toBe("供应商导入失败行.xlsx")
    const downloadPath = testInfo.outputPath("供应商失败行.xlsx")
    await download.saveAs(downloadPath)
    const workbook = await readImportWorkbook(downloadPath)
    const failureSheet = workbook.getWorksheet("失败行")!
    expect(failureSheet, "下载必须是可打开的失败工作簿").toBeTruthy()
    expect(failureSheet.rowCount).toBe(3)
    expect([
        failureSheet.getCell("B2").text,
        failureSheet.getCell("B3").text,
    ]).toEqual([invalidName, formulaName])
    expect([
        failureSheet.getCell("X2").text,
        failureSheet.getCell("X3").text,
    ]).toEqual(["4", "5"])
    expect(failureSheet.getCell("Y2").text).toContain("缺少结算方式")
    expect(failureSheet.getCell("Z3").text).toContain("导入单元格不能包含公式")
    expect(typeof failureSheet.getCell("E3").value).toBe("string")
    expect(failureSheet.getCell("E3").text).toContain("【待修正】")
    expect(failureSheet.getCell("E2").text).toBe("00123456789012345678")

    const repeated = await context.request.post(
        `${API_BASE}/admin/supplier-profiles/import/jobs`,
        {
            headers: { Authorization: `Bearer ${token}` },
            data: requestBody,
        },
    )
    expect(repeated.ok()).toBeTruthy()
    expect((await repeated.json()).data.id).toBe(job.id)
    const changed = await context.request.post(
        `${API_BASE}/admin/supplier-profiles/import/jobs`,
        {
            headers: { Authorization: `Bearer ${token}` },
            data: { ...requestBody, file_name: "改变文件名.xlsx" },
        },
    )
    expect(changed.status(), "相同提交身份不得用于另一份导入内容").toBe(409)
    const otherToken = await apiToken("caigou")
    const forbidden = await context.request.get(
        `${API_BASE}/admin/supplier-profiles/import/jobs/${job.id}/failures`,
        { headers: { Authorization: `Bearer ${otherToken}` } },
    )
    expect(
        forbidden.status(),
        "具备导入权限的其他员工也不得下载原提交人的敏感源行",
    ).toBe(403)
    expect((await forbidden.json()).success).toBe(false)
})
