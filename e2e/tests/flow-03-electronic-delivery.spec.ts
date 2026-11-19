/**
 * 流程: [flow-03] 虚拟商品电子交付
 * 文档: docs/erp-phase-1.md §7.3.3 + §7.4（供给分配）+ §6.2 电子交付记录
 * 账号: admin（目录补齐）→ xiaoshou（客户/合同/销售单/验收）
 *       → caigou（采购确认、供给分配、电子交付）→ caigoushenpi（采购单审批）
 *
 * 验收：采购生效生成电子交付任务；采购提交对象、实际时间、结果与图片凭证；
 *       销售登记客户验收。交付不走审批，不影响自有库存。
 * 目录种子无 VIRTUAL SKU 时，通过 UI 补齐本流程使用的商品。
 */
import { workPage } from "../helpers/workspace-page"
import { archiveContractViaUi } from "../helpers/contracts"
import { test, expect, type Locator, type Page } from '../helpers/test'
import fs from 'node:fs'
import path from 'node:path'

import { toAutomationIdSegment } from '../../erp-client/lib/automation-id'
import { API_BASE, apiGet, apiToken } from '../helpers/api'
import { createCustomerViaUi } from '../helpers/customers'
import { approveSourcedPurchase } from '../helpers/financial-analytics'
import { openFulfillmentTaskForSource, uploadAcceptanceEvidence } from '../helpers/fulfillment'
import { openLoggedInWorkspace } from '../helpers/login'
import { BANK_RECEIPT_FILE, payOnlySupplierTask } from '../helpers/payments'
import { ensureDefaultProcurementOwner } from '../helpers/procurement'
import { fillSalesLineUnitPrice } from '../helpers/sales-orders'
import { confirmSupplyAllocation } from '../helpers/sourcing'
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    expectSourcingCounts,
    workspaceTaskId,
} from '../helpers/ui'

test.describe.configure({ mode: 'serial' })

// 供应商订货编码；公司商品与 SKU 编码由系统生成，不再作为查找键。
const VIRTUAL_SUPPLIER_SKU_CODE = 'E2E-VIRT-ED-001'
const VIRTUAL_PRODUCT_NAME = 'E2E 电子卡密（虚拟）'
const VIRTUAL_CATEGORY_NAME = 'E2E虚拟商品'
const PNG_1X1 = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=',
    'base64',
)

async function clickWithoutToastOverlay(page: Page, target: Locator) {
    // Toast 可能在关闭后再次出现导致遮挡：循环关闭后短超时点按，成功即返回。
    for (let i = 0; i < 8; i += 1) {
        await dismissToasts(page)
        try {
            await target.click({ timeout: 3_000 })
            return
        } catch {
            // 被遮挡则下一轮重试；8 轮都不成功改走 DOM 派发。
        }
    }
    // 悬浮提示持续遮挡导致真实点击无法命中：改走 DOM 直接派发点击，绕过覆盖层。
    await target.dispatchEvent('click')
}

async function chooseComboboxById(page: Page, id: string, optionLabel: string | RegExp) {
    await chooseOption(page, workPage(page).locator(`#${id}`), optionLabel)
}

function isoDate(offsetDays = 0): string {
    const date = new Date()
    date.setDate(date.getDate() + offsetDays)
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function contractPdfPath(): { name: string; mimeType: string; buffer: Buffer } {
    const candidates = [
        path.join(process.cwd(), 'fixtures', 'sample-contract.pdf'),
        path.join(process.cwd(), '..', 'fixtures', 'sample-contract.pdf'),
    ]
    for (const filePath of candidates) {
        if (fs.existsSync(filePath)) {
            return {
                name: 'sample-contract.pdf',
                mimeType: 'application/pdf',
                buffer: fs.readFileSync(filePath),
            }
        }
    }
    return {
        name: 'sample-contract.pdf',
        mimeType: 'application/pdf',
        buffer: Buffer.from('%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n'),
    }
}

function virtualCategoryTreeItem(page: Page) {
    return workPage(page)
        .getByRole('complementary', { name: '分类导航' })
        .getByRole('treeitem', { name: new RegExp(`^${VIRTUAL_CATEGORY_NAME}(?:，|$)`) })
}

async function expectVirtualCategoryOpen(page: Page) {
    await expect(workPage(page).getByRole('heading', { name: VIRTUAL_CATEGORY_NAME, exact: true })).toBeVisible({
        timeout: 20000,
    })
    await expect(workPage(page).getByLabel('分类路径').locator('[aria-current="page"]')).toHaveText(
        VIRTUAL_CATEGORY_NAME,
    )
}

async function closeNewCategoryDialog(page: Page) {
    const dialog = workPage(page).getByRole('dialog', { name: '新建商品分类' })
    const discard = workPage(page).getByRole('alertdialog', { name: '放弃未保存的更改？' })
    // 表单已填过：直接关会先弹出放弃确认；确认层已经在上面时不再点被挡住的关闭。
    if (!(await discard.isVisible().catch(() => false))) {
        await dialog.getByRole('button', { name: '关闭' }).click()
    }
    await expect(discard).toBeVisible({ timeout: 20000 })
    await discard.getByRole('button', { name: '放弃更改' }).click()
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(discard).toBeHidden({ timeout: 20000 })
}

async function openExistingVirtualCategory(page: Page) {
    const nav = workPage(page).getByRole('complementary', { name: '分类导航' })
    if ((await virtualCategoryTreeItem(page).count()) !== 1) {
        const expand = nav.locator('#category-nav-expand')
        if (await expand.isVisible().catch(() => false)) await expand.click()
    }
    if ((await virtualCategoryTreeItem(page).count()) !== 1) {
        const search = nav.getByLabel('搜索分类名称或代码')
        await search.fill(VIRTUAL_CATEGORY_NAME)
        await search.press('Enter')
    }
    const item = virtualCategoryTreeItem(page)
    await expect(item).toHaveCount(1, { timeout: 20000 })
    await item.click()
    await expectVirtualCategoryOpen(page)
}

async function ensureVirtualCategory(page: Page) {
    // 分类树在标题之后加载，先等列表接口返回再判断，避免重复创建触发 409。
    // 先挂监听再导航：请求常在标题出现前就已返回，事后再等只会空等到超时。
    const categoriesLoaded = page
        .waitForResponse(
            (response) =>
                response.request().method() === 'GET' &&
                response.url().includes('product-categories'),
            { timeout: 20000 },
        )
        .catch(() => undefined)
    await page.goto('/master-data/categories')
    await expect(workPage(page).getByRole('heading', { name: '商品分类', exact: true })).toBeVisible({ timeout: 20000 })
    await categoriesLoaded
    const nav = workPage(page).getByRole('complementary', { name: '分类导航' })
    await expect(nav.getByText('正在加载分类…')).toBeHidden({ timeout: 20000 })
    if ((await virtualCategoryTreeItem(page).count()) === 1) {
        await virtualCategoryTreeItem(page).click()
        await expectVirtualCategoryOpen(page)
        return
    }
    await workPage(page).locator('#master-data-category-tree-create-root').click()
    const dialog = workPage(page).getByRole('dialog', { name: /新建商品分类/ })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.locator('#master-data-category-create-dialog-name').fill(VIRTUAL_CATEGORY_NAME)
    await chooseComboboxById(page, 'master-data-category-create-dialog-product-kind', '虚拟')
    await dialog.locator('#master-data-category-create-dialog-change-reason').fill('E2E 电子交付目录')
    await dialog.locator('#master-data-category-create-dialog-submit').click()
    // 分类主数据清库后仍在。重复提交可能是「已新建」，也可能停在冲突文案里。
    const createdCategory = workPage(page).locator('[data-slot="toast-title"]').filter({ hasText: '已新建' })
    const conflictCategory = workPage(page).getByText('分类已更新，请重新打开后修改')
    const duplicateCategory = workPage(page).getByText('数据已存在，请勿重复提交')
    await expect(createdCategory.or(conflictCategory).or(duplicateCategory).first()).toBeVisible({
        timeout: 20000,
    })
    const sawCreated = await createdCategory.isVisible().catch(() => false)
    const sawExisting =
        (await conflictCategory.isVisible().catch(() => false)) ||
        (await duplicateCategory.isVisible().catch(() => false))
    if (sawCreated && !sawExisting) {
        await expectToast(page, '已新建')
        await expectVirtualCategoryOpen(page)
        return
    }
    await closeNewCategoryDialog(page)
    await openExistingVirtualCategory(page)
}

async function ensureVirtualProduct(page: Page) {
    await page.goto('/master-data/products')
    await expect(workPage(page).getByRole('heading', { name: '商品列表', exact: true, level: 1 })).toBeVisible({ timeout: 20000 })
    const search = workPage(page).locator('#master-data-products-list-toolbar-search-input')
    await search.fill(VIRTUAL_PRODUCT_NAME)
    const searched = page.waitForResponse((response) => {
        const url = new URL(response.url())
        return response.request().method() === 'GET' &&
            url.pathname === '/admin/products' &&
            url.searchParams.get('keyword') === VIRTUAL_PRODUCT_NAME
    })
    await search.press('Enter')
    const searchResponse = await searched
    const searchResult = await searchResponse.json()
    expect(searchResponse.ok(), JSON.stringify(searchResult)).toBe(true)
    const existing = workPage(page).getByRole('row').filter({ hasText: VIRTUAL_PRODUCT_NAME })
    if (searchResult.data.items.length > 0) {
        await expect(existing).toHaveCount(1)
        await existing.getByText(VIRTUAL_PRODUCT_NAME, { exact: true }).click()
        await expect(workPage(page).getByRole('heading', { name: '规格与 SKU', exact: true })).toBeVisible({ timeout: 20000 })
        return
    }
    await workPage(page).locator('#master-data-products-list-create').click()
    // 详情页各分区使用 detail 前缀 id（basic 分区为 master-data-product-detail-basic）。
    const basicId = 'master-data-product-detail-basic'
    // 商品编号由系统在保存时生成，新建页不再提供编号输入。
    await expect(workPage(page).locator(`#${basicId}-name`)).toBeVisible({
        timeout: 20000,
    })
    await workPage(page).locator(`#${basicId}-name`).fill(VIRTUAL_PRODUCT_NAME)
    await workPage(page).locator(`#${basicId}-description`).fill('E2E 虚拟商品电子交付用')
    await chooseComboboxById(page, `${basicId}-kind-combobox`, '虚拟')
    await chooseComboboxById(page, `${basicId}-unit-combobox`, /张/)
    await chooseComboboxById(page, `${basicId}-category-combobox`, VIRTUAL_CATEGORY_NAME)
    await chooseComboboxById(page, `${basicId}-brand-combobox`, /福尚云/)
    // SKU 与商品资料同页编辑，使用稳定分区定位。
    await workPage(page).locator('#product-section-sku').scrollIntoViewIfNeeded()
    // SKU 名称、条码与价格在 SKU 表格行内直接填写；SKU 编码保存后由系统按商品编号生成。
    await workPage(page).getByLabel('默认规格 SKU 名称').fill(VIRTUAL_PRODUCT_NAME)
    // 主图在表格行内 tile 上传（启用 SKU 必填）：点行内「选择主图」走文件选择器。
    const imageGroup = workPage(page).getByRole('group', { name: /主图/ }).first()
    const [chooser] = await Promise.all([
        page.waitForEvent('filechooser', { timeout: 20000 }),
        imageGroup.getByRole('button', { name: '选择主图' }).click(),
    ])
    await chooser.setFiles({
        name: 'virtual-sku.png',
        mimeType: 'image/png',
        buffer: PNG_1X1,
    })
    await workPage(page).getByLabel('默认规格 一件代发价').fill('100.00')
    await workPage(page).getByLabel('默认规格 市场价').fill('120.00')
    await workPage(page).locator('#master-data-product-detail-header-submit').click()
    // 保存改为两步确认：先在「创建商品」框填写变更原因，再确认保存。
    const saveDialog = workPage(page).getByRole('dialog', { name: '创建商品' })
    await expect(saveDialog).toBeVisible({ timeout: 20000 })
    await saveDialog.locator('#master-data-product-detail-effective-reason').fill('E2E 电子交付新建商品')
    const saved = page.waitForResponse((response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === '/admin/products/with-assets',
    )
    await saveDialog.locator('#master-data-product-save-confirm').click()
    const saveResponse = await saved
    const created = await saveResponse.json()
    expect(saveResponse.ok(), JSON.stringify(created)).toBe(true)
    expect(created.data).toMatchObject({ product_kind: 'VIRTUAL', sku_count: 1 })
    expect(created.data.id).toBeTruthy()
    await expect(page).toHaveURL(new RegExp(`/master-data/products/${created.data.id}(?:[?#]|$)`), { timeout: 20000 })
    await expect(saveDialog).toBeHidden({ timeout: 20000 })
    await expect(workPage(page).getByRole('heading', { name: '规格与 SKU', exact: true })).toBeVisible({ timeout: 20000 })
}

async function ensureVirtualOfferingAndListing(page: Page) {
    // 供给列表在标题之后加载，先等列表接口返回再判断，避免重复登记。先挂监听再导航。
    const offeringsLoaded = page
        .waitForResponse(
            (response) =>
                response.request().method() === 'GET' &&
                response.url().includes('supplier-offerings'),
            { timeout: 20000 },
        )
        .catch(() => undefined)
    await page.goto('/procurement/supplier-offerings')
    await expect(workPage(page).getByRole('heading', { name: '供应商供给' })).toBeVisible({
        timeout: 20000,
    })
    await offeringsLoaded
    if (!(await workPage(page).getByText(VIRTUAL_SUPPLIER_SKU_CODE).count())) {
        await workPage(page).locator('#supplier-offerings-page-create').click()
        const dialog = workPage(page).getByRole('dialog', { name: '添加供给' })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        await chooseComboboxById(
            page,
            'supplier-offerings-dialog-register-sku',
            VIRTUAL_PRODUCT_NAME,
        )
        await chooseComboboxById(page, 'supplier-offerings-dialog-register-supplier', /上海通卡/)
        await dialog.locator('#supplier-offerings-dialog-register-supplier-sku-code').fill(VIRTUAL_SUPPLIER_SKU_CODE)
        await dialog.locator('#supplier-offerings-dialog-register-dropship-price').fill('88.00')
        await dialog.locator('#supplier-offerings-dialog-register-bulk-price').fill('80.00')
        await dialog.locator('#supplier-offerings-dialog-register-minimum-quantity').fill('1')
        await dialog.locator('#supplier-offerings-dialog-register-input-tax-percentage').fill('6')
        await pickCalendarDay(page, workPage(page).locator('#supplier-offerings-dialog-register-valid-from'), isoDate(0))
        await dialog.locator('#supplier-offerings-dialog-register-available-quantity').fill('1000')
        await dialog.locator('#supplier-offerings-dialog-register-submit').click()
        // 主数据在重置间保留，供给可能已登记：成功与重复均为合法结果。
        // 重复时提示和表单说明各有一句「已登记供给」，合并定位后再取第一条。
        await expect(workPage(page).getByText(/供给已添加|已登记供给/).first()).toBeVisible({
            timeout: 20000,
        })
        if (await workPage(page).getByText('已登记供给').count()) {
            // 409 本身证明供给已存在，直接复用，不依赖列表分页可见性。
            await dialog.getByRole('button', { name: '关闭' }).first().click()
            await workPage(page).getByRole('alertdialog').getByRole('button', { name: '放弃更改' }).click()
            await expect(dialog).toBeHidden({ timeout: 20000 })
        } else {
            await expectToast(page, '供给已添加')
            await expect(dialog).toBeHidden({ timeout: 20000 })
            await expect(workPage(page).getByText(VIRTUAL_SUPPLIER_SKU_CODE).first()).toBeVisible({ timeout: 20000 })
        }
    }

    await page.goto('/master-data/products')
    await expect(workPage(page).getByRole('heading', { name: '商品列表', exact: true, level: 1 })).toBeVisible({ timeout: 20000 })
    const search = workPage(page).locator('#master-data-products-list-toolbar-search-input')
    await search.fill(VIRTUAL_PRODUCT_NAME)
    await search.press('Enter')
    const listing = workPage(page).getByRole('switch', { name: new RegExp(`^${VIRTUAL_PRODUCT_NAME}整组上架状态`) })
    await expect(listing).toBeVisible({ timeout: 20000 })
    if (!(await listing.isChecked())) {
        await listing.click()
        await expect(listing).toBeChecked({ timeout: 20000 })
    }
}

async function uploadContractOnSalesOrder(page: Page, legalName: string, contractNo: string) {
    await archiveContractViaUi(page, { contractNo, customerName: legalName, pdf: contractPdfPath(), paymentTerms: "货到 30 天" })
    await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(new RegExp(legalName))
}

const ELECTRONIC_SUPPLIER = '上海通卡'

async function expectElectronicPaymentReceipt(
    page: Page,
    deliveryDialog: Locator,
    purchaseId: string,
    payment: Awaited<ReturnType<typeof payOnlySupplierTask>>,
) {
    const taskId = workspaceTaskId(page)
    expect(taskId).toBeTruthy()
    const receiptPath = `/admin/work-items/${taskId}/payment-receipts`
    const token = await apiToken('caigou')
    const files = await apiGet<Array<Record<string, unknown>>>(token, receiptPath)
    expect(files).toEqual([expect.objectContaining({
        document_id: payment.id,
        document_no: payment.payment_no,
        file_asset_id: payment.bank_receipt.asset_id,
        file_name: BANK_RECEIPT_FILE.name,
        content_type: BANK_RECEIPT_FILE.mimeType,
        byte_size: BANK_RECEIPT_FILE.buffer.length,
    })])

    const unrelated = await page.request.get(`${API_BASE}${receiptPath}/payment-not-linked-to-task/download`, {
        headers: { Authorization: `Bearer ${token}` },
    })
    expect(unrelated.status()).toBe(404)

    const paymentLink = deliveryDialog.locator('#fulfillment-operations-gate-view-payment')
    await expect(paymentLink).toHaveAttribute('href', `/procurement/orders/${purchaseId}?section=payable`)
    const [purchasePage] = await Promise.all([page.waitForEvent('popup'), paymentLink.click()])
    try {
        await purchasePage.waitForURL(`**/procurement/orders/${purchaseId}?section=payable`)
        const row = workPage(purchasePage).getByRole('region', { name: '财务付款单', exact: true }).getByRole('listitem')
        await expect(row).toHaveCount(1)
        await expect(row.getByText(payment.payment_no, { exact: true })).toBeVisible()
        const [download, response] = await Promise.all([
            purchasePage.waitForEvent('download'),
            purchasePage.waitForResponse(result => new URL(result.url()).pathname === `/admin/purchase-orders/${purchaseId}/payments/${payment.id}/receipt`),
            row.getByRole('button', { name: '下载', exact: true }).click(),
        ])
        expect(response.ok()).toBe(true)
        expect(response.headers()['content-type']).toBe(BANK_RECEIPT_FILE.mimeType)
        expect(response.headers()['cache-control']).toBe('private, no-store')
        expect(download.suggestedFilename()).toBe(BANK_RECEIPT_FILE.name)
        expect(await download.failure()).toBeNull()
        expect(fs.readFileSync((await download.path())!)).toEqual(BANK_RECEIPT_FILE.buffer)
    } finally {
        await purchasePage.close()
    }
    expect(workspaceTaskId(page)).toBe(taskId)
}

async function pickVirtualSku(page: Page) {
    await workPage(page).locator("#sales-orders-create-line-items-add").click()
    const dialog = workPage(page).getByRole('dialog', { name: '添加商品' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await chooseComboboxById(page, 'sales-orders-sku-picker-filter-kind', '虚拟')
    await dialog.locator('#sales-orders-sku-picker-filter-query').click()
    const empty = dialog.getByText('当前筛选无结果')
    if (await empty.isVisible().catch(() => false)) {
        throw new Error('公司商品池没有可销售的虚拟 SKU，目录补齐未生效')
    }
    const rowName = new RegExp(VIRTUAL_PRODUCT_NAME)
    await expect(dialog.getByText(rowName).first()).toBeVisible({ timeout: 20000 })
    await dialog.getByRole('checkbox', { name: new RegExp(`选择 .*${VIRTUAL_PRODUCT_NAME}`) }).click()
    await dialog.locator('#sales-orders-sku-picker-confirm').click()
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(workPage(page).getByText(VIRTUAL_PRODUCT_NAME).first()).toBeVisible({ timeout: 20000 })
}

test('虚拟商品电子交付全流程：销售单生效后只能采购、登记电子交付并验收', async ({ browser }) => {
    test.setTimeout(8 * 60 * 1000)
    const stamp = Date.now().toString(36).slice(-6)
    const customerName = `E2E虚拟客户${stamp}有限公司`
    const creditCode = `91110105E2E${stamp}XX`.slice(0, 18).padEnd(18, '0')
    const contractNo = `HT-E2E-ED-${stamp}`
    const due = isoDate(45)
    const orderedQuantity = '10'
    let salesOrderId = ''
    let supplierPayment: Awaited<ReturnType<typeof payOnlySupplierTask>>
    let salesOrderNo = ''
    let purchase: { purchase_order_id: string; purchase_no: string }
    let electronicDeliveryId: string

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'admin')
        try {
            await ensureDefaultProcurementOwner(page)
            await ensureVirtualCategory(page)
            await ensureVirtualProduct(page)
            await ensureVirtualOfferingAndListing(page)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'xiaoshou')
        try {
            await createCustomerViaUi(page, {
                legalName: customerName,
                shortName: 'E2E虚拟客户',
                creditCode,
                paymentTermLabel: '货到 15 天',
                contact: { name: '李测', phone: '13800138001' },
                address: '北京市朝阳区测试路 1 号',
            })
            // 列表行链接展示客户简称，非法定全称。
            await expect(workPage(page).getByRole('link', { name: 'E2E虚拟客户' })).toBeVisible({ timeout: 20000 })
            await page.goto('/sales/orders?mode=create')
            await uploadContractOnSalesOrder(page, customerName, contractNo)
            await expect(workPage(page).locator('#sales-orders-create-header-nature')).toBeVisible({
                timeout: 20000,
            })
            await chooseComboboxById(page, 'sales-orders-create-header-welfare-scene', '礼包')
            await expect(workPage(page).locator("#sales-orders-create-header-payment-terms")).toContainText("货到 30 天")
            await expect(workPage(page).locator("#sales-orders-create-header-payment-terms").locator("input, button, [role=combobox]")).toHaveCount(0)
            await pickVirtualSku(page)
            await workPage(page).locator('[id^="sales-orders-create-line-"][id$="-quantity"]').first().fill(orderedQuantity)
            await fillSalesLineUnitPrice(page, '100.00')
            await workPage(page).locator("#sales-orders-create-batch-due-date-open").click()
            await pickCalendarDay(page, workPage(page).locator('#sales-orders-create-batch-due-date'), due)
            await workPage(page).locator('#sales-orders-create-batch-due-date-apply').click()
            await expectToast(page, '已批量设置交期')
            await expect(workPage(page).getByTestId(/sales-line-procurement-owner-/)).not.toHaveText(
                /暂未确定采购负责人/,
                { timeout: 20000 },
            )
            await workPage(page).getByTestId('sales-order-submit').click()
            const submitDialog = workPage(page).getByRole('dialog', { name: '提交销售单' })
            await expect(submitDialog).toBeVisible({ timeout: 20000 })
            await expect(submitDialog.getByText('审批中')).toBeVisible()
            await clickWithoutToastOverlay(page, submitDialog.locator('#sales-orders-submit-confirm-confirm'))
            await expect(submitDialog).toBeHidden({ timeout: 30000 })
            await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: 30000 })
            salesOrderId = page.url().split('/sales/orders/')[1]?.split('?')[0] ?? ''
            expect(salesOrderId).toBeTruthy()
            salesOrderNo = await readHeaderDocumentNumber(page)
            await expect(workPage(page).getByText(/审批中/).first()).toBeVisible({ timeout: 20000 })
            await expect(workPage(page).locator('#sales-order-follow-up-delivery').locator("..")).toContainText('待采购', {
                timeout: 20000,
            })
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'caigou')
        try {
            await openWorkspaceTask(page, /销售单审批/, salesOrderNo, 'approval')
            await expect(workPage(page).getByRole('button', { name: /^(通过|通过审批)$/ })).toBeVisible({ timeout: 20000 })
            await expect(workPage(page).getByText('采购确认').first()).toBeVisible({ timeout: 20000 })
            await approveCurrentDocument(page)
            await openWorkspaceTask(page, /待安排采购/, salesOrderNo, 'procurement')
            await expect(workPage(page).getByRole('heading', { name: '供给分配' })).toBeVisible({
                timeout: 20000,
            })
            await expect(workPage(page).getByRole('region', { name: '供给商品列表' }).getByRole('button', { name: `调整 ${VIRTUAL_PRODUCT_NAME}`, exact: true })).toBeVisible({ timeout: 20000 })
            await expect(workPage(page).getByText('现有库存').filter({ visible: true })).toHaveCount(0)
            await expect(workPage(page).getByText('电子交付').filter({ visible: true }).first()).toBeVisible({
                timeout: 20000,
            })
            await expectSourcingCounts(page, 1, 0)
            await workPage(page).getByTestId('purchase-create-preview').click()
            const preview = workPage(page).getByRole('dialog', { name: '预览供给分配' })
            await expect(preview).toBeVisible({ timeout: 20000 })
            await expect(preview.getByText('现有库存分配')).toHaveCount(0)
            await expect(preview.getByText('电子交付').first()).toBeVisible({ timeout: 20000 })
            const allocation = await confirmSupplyAllocation(page)
            expect(allocation.orders).toHaveLength(1)
            expect(allocation.stock_reservations).toEqual([])
            expect(allocation.work_item_status).toBe('COMPLETED')
            purchase = allocation.orders[0]!
            expect(purchase.purchase_order_id).toBeTruthy()
            expect(purchase.purchase_no).toMatch(/^PO-/)
            await expect(preview).toBeHidden()
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'xiaoshou')
        try {
            await page.goto(`/sales/orders/${salesOrderId}`)
            await expect(workPage(page).getByText(/已生效/).first()).toBeVisible({ timeout: 20000 })
            await expect(workPage(page).locator('#sales-order-follow-up-delivery').locator("..")).toContainText('采购已覆盖', {
                timeout: 20000,
            })
            expect(await apiGet<{ purchase_order_count: number }>(
                await apiToken("xiaoshou"), `/admin/sales-orders/${salesOrderId}`,
            )).toMatchObject({ purchase_order_count: 1 })
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'caigoushenpi')
        try {
            await approveSourcedPurchase(page, purchase!.purchase_order_id)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'fukuan')
        try {
            supplierPayment = await payOnlySupplierTask(page, purchase!.purchase_no)
            expect(supplierPayment.allocations.map(row => row.source_document_no)).toEqual([purchase!.purchase_no])
        } finally { await context.close() }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'caigou')
        try {
            await page.goto('/procurement/orders')
            await expect(workPage(page).getByRole('heading', { name: '采购单', exact: true })).toBeVisible({ timeout: 20000 })
            const purchaseId = purchase!.purchase_order_id
            const purchaseNo = purchase!.purchase_no
            const poSearch = workPage(page).locator('#procurement-orders-list-search')
            await poSearch.fill(purchaseNo)
            await poSearch.press('Enter')
            const rowPrefix = `procurement-orders-list-row-${toAutomationIdSegment(purchaseId)}`
            const openPo = workPage(page).locator(`#${rowPrefix}-open`)
            const orderRow = workPage(page).getByRole('row').filter({ has: openPo })
            await expect(orderRow).toBeVisible({ timeout: 20000 })
            await expect(openPo).toHaveAttribute('href', `/procurement/orders/${purchaseId}`)
            await expect(openPo).toHaveText(purchaseNo)
            await expect(orderRow.locator(`#${rowPrefix}-sales-order`)).toHaveText(salesOrderNo)
            await expect(orderRow).toContainText(ELECTRONIC_SUPPLIER)
            await expect(orderRow.getByText('虚拟 / 电子交付', { exact: true })).toBeVisible()
            await expect(orderRow.getByText('已生效', { exact: true })).toBeVisible()

            await openFulfillmentTaskForSource(page, { salesOrderId, purchaseOrderId: purchaseId, operation: 'ELECTRONIC' })
            const deliveryDialog = await openFulfillmentWorkspaceForm(page)
            await expectElectronicPaymentReceipt(page, deliveryDialog, purchaseId, supplierPayment!)
            const electronicForm = deliveryDialog.getByRole('region', { name: '电子交付表单', exact: true })
            await expect(electronicForm.getByText(`对应销售单 ${salesOrderNo}`, { exact: true })).toBeVisible({ timeout: 20000 })
            await expect(deliveryDialog.getByRole('button', { name: /^(通过|通过审批|同意审批)$/ })).toHaveCount(0)
            await electronicForm.getByRole('textbox', { name: '交付对象' }).fill('E2E 客户企业邮箱收件人')
            await electronicForm.locator('#fulfillment-operations-electronic-form-evidence-input').setInputFiles({ name: 'electronic-delivery.png', mimeType: 'image/png', buffer: PNG_1X1 })
            await chooseOption(page, electronicForm.locator('#fulfillment-operations-electronic-form-result'), '成功')
            const quantity = await electronicForm.getByRole('textbox', { name: '交付数量' }).inputValue()
            expect(Number(quantity)).toBe(Number(orderedQuantity))
            await workPage(page).locator('#fulfillment-operations-work-surface-confirm').click()
            const confirm = workPage(page).getByRole('alertdialog', { name: '确认交付？' })
            await expect(confirm).toBeVisible({ timeout: 20000 })
            const responsePromise = page.waitForResponse(response => response.request().method() === 'POST' && /\/admin\/electronic-deliveries\/[^/]+\/confirm$/.test(response.url()))
            await confirm.getByRole('button', { name: '确认交付', exact: true }).click()
            const response = await responsePromise
            const body = await response.json()
            expect(response.ok(), JSON.stringify(body)).toBeTruthy()
            expect(body.data).toMatchObject({ status: 'CONFIRMED', result: 'SUCCESS', quantity, purchase_order_id: purchaseId })
            electronicDeliveryId = body.data.id
            expect(electronicDeliveryId).toBeTruthy()
            // 成功交付按正式采购冻结分配入实际商品成本，归属稳定销售行。
            const adminToken = await apiToken('admin')
            const persistedPurchase = await apiGet<{
                allocations: Array<{
                    id: string
                    allocated_quantity: string
                    allocated_cost_gross: string
                    allocated_cost_net: string
                }>
            }>(adminToken, `/admin/purchase-orders/${purchaseId}`)
            const allocation = persistedPurchase.allocations.find(row => row.id === body.data.purchase_line_sales_allocation_id)
            expect(allocation).toBeDefined()
            expect(allocation!.allocated_quantity).toBe(quantity)
            const sales = await apiGet<{ lines: Array<{ id: string }> }>(adminToken, `/admin/sales-orders/${salesOrderId}`)
            expect(sales.lines.map(row => row.id)).toContain(body.data.sales_order_line_id)
            const costs = await apiGet<{ items: Array<{
                cost_type: string
                cost_stage: string
                source_fact_type: string
                source_document_id: string
                source_line_id: string
                gross_amount: string
                net_amount: string
                allocations: Array<Record<string, unknown>>
            }> }>(adminToken, '/admin/cost-entries', { source_document_id: body.data.purchase_order_id, cost_stage: 'actual' })
            expect(costs.items).toHaveLength(1)
            expect(costs.items[0]).toMatchObject({
                cost_type: 'product', cost_stage: 'actual', source_fact_type: 'purchase_fulfillment',
                source_document_id: body.data.purchase_order_id, source_line_id: allocation!.id,
                gross_amount: allocation!.allocated_cost_gross, net_amount: allocation!.allocated_cost_net,
            })
            expect(costs.items[0]!.allocations).toEqual([expect.objectContaining({
                sales_order_id: salesOrderId, sales_order_line_id: body.data.sales_order_line_id,
                allocated_gross_amount: allocation!.allocated_cost_gross, allocated_net_amount: allocation!.allocated_cost_net,
            })])
            await expect(confirm).toBeHidden({ timeout: 20000 })

        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openLoggedInWorkspace(browser, 'xiaoshou')
        try {
            await openWorkspaceTask(page, /客户验收登记/, salesOrderNo, 'fulfillment')
            const acceptanceTaskId = workspaceTaskId(page)
            expect(acceptanceTaskId).toBeTruthy()
            await workPage(page).locator('#sales-orders-acceptance-register-open').click()
            const registerDialog = workPage(page).getByRole('dialog', { name: '登记客户验收', exact: true })
            await expect(registerDialog).toBeVisible({ timeout: 20000 })
            const deliveryBatch = registerDialog.getByRole('article').filter({
                has: workPage(page).locator(`#sales-orders-acceptance-batch-${toAutomationIdSegment(electronicDeliveryId!)}-include`),
            })
            await expect(deliveryBatch.getByText(/^电子交付/)).toBeVisible()
            await uploadAcceptanceEvidence(page)
            await workPage(page).locator('#sales-orders-acceptance-register-submit').click()
            await expect(workPage(page).getByRole('heading', { name: '确认客户验收' })).toBeVisible({ timeout: 20000 })
            const accepted = page.waitForResponse(response => response.request().method() === 'POST' && new URL(response.url()).pathname === '/admin/customer-acceptances/commit', { timeout: 90_000 })
            await workPage(page).locator('#sales-orders-acceptance-confirm-confirm').click()
            const acceptanceResponse = await accepted
            expect(acceptanceResponse.ok(), await acceptanceResponse.text()).toBeTruthy()
            const command = acceptanceResponse.request().postDataJSON()
            expect(command.work_item_id).toBe(acceptanceTaskId)
            expect(command.evidence_attachment_id).toBeTruthy()
            expect(command.lines).toHaveLength(1)
            expect(command.lines[0].allocations).toEqual([expect.objectContaining({
                fulfillment_fact_type: 'ELECTRONIC_DELIVERY',
                fulfillment_line_id: electronicDeliveryId!,
            })])
            const { acceptance } = (await acceptanceResponse.json()).data
            expect(acceptance).toMatchObject({
                sales_order_id: salesOrderId,
                evidence_attachment_id: command.evidence_attachment_id,
                result: 'PASSED',
                status: 'POSTED',
            })
            expect(acceptance.acceptance_no).toBeTruthy()
            await expect(registerDialog).toBeHidden({ timeout: 20000 })
            await expect.poll(() => workspaceTaskId(page)).not.toBe(acceptanceTaskId)

            await page.goto(`/sales/orders/${salesOrderId}`)
            await workPage(page).getByRole('tab', { name: /^交付与验收/ }).click()
            await workPage(page).locator('#sales-order-section-acceptance').click()
            const progress = workPage(page).getByRole('table').filter({
                has: workPage(page).getByRole('columnheader', { name: '待验收', exact: true }),
            })
            const row = progress.getByRole('row', { name: new RegExp(VIRTUAL_PRODUCT_NAME) })
            await expect(row.getByRole('cell').nth(2)).toHaveText(new RegExp(`^${orderedQuantity}(?:\\s|$)`))
            await expect(row.getByRole('cell').nth(3)).toHaveText(new RegExp(`^${orderedQuantity}(?:\\s|$)`))
            await expect(row.getByRole('cell').nth(4)).toHaveText(/^0(?:\s|$)/)
            const evidenceDownload = workPage(page).locator(`#sales-orders-acceptance-history-${toAutomationIdSegment(acceptance.id)}-download`)
            await expect(evidenceDownload).toHaveAccessibleName('下载签收单凭证')
            await expect(evidenceDownload).toBeVisible()
            await expect(workPage(page).locator('#sales-orders-acceptance-register-open')).toHaveCount(0)
            await expect(workPage(page).locator('#sales-order-follow-up-delivery').locator('..')).toContainText('采购已覆盖')
        } finally {
            await context.close()
        }
    }

    expect(salesOrderNo || salesOrderId).toBeTruthy()
})
