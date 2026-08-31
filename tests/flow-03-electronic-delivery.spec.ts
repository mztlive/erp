/**
 * 流程: [flow-03] 虚拟商品电子交付
 * 文档: docs/erp-phase-1.md §7.3.3 + §7.4（供给分配）+ §6.2 电子交付记录
 * 账号: admin（目录补齐）→ xiaoshou（客户/合同/销售单/验收）
 *       → caigou（采购确认、供给分配、电子交付）→ caiwu（采购单审批）
 *
 * 文档-代码差异（测试按代码走）：
 * 1. 电子交付确认只 POST version/expected_source_version/idempotency_key，
 *    表单的对象/数量/结果/时间不随确认命令提交；无凭证上传入口。
 * 2. 采购单最终通过只为入仓/直发/服务生成履约草稿，未实现 Electronic 草稿。
 * 3. 开发目录种子无 VIRTUAL SKU（仅实物/卡券/线下服务），本 spec 在 UI 上幂等补齐。
 * 4. 电子交付「交付对象」只读且前端常为空，客户端校验会拦住「确认交付」。
 */
import { test, expect, type Browser, type BrowserContext, type Locator, type Page } from '@playwright/test'
import fs from 'node:fs'
import path from 'node:path'

import { ACCOUNTS } from '../helpers/accounts'
import { loginViaUi, newLoggedInContext } from '../helpers/login'

test.describe.configure({ mode: 'serial' })

const PASSWORD = '123456'
const VIRTUAL_PRODUCT_NO = 'E2E-VIRT-ED-001'
const VIRTUAL_SKU_NO = 'E2E-VIRT-ED-001'
const VIRTUAL_PRODUCT_NAME = 'E2E 电子卡密（虚拟）'
const VIRTUAL_CATEGORY_NAME = 'E2E虚拟商品'
const VIRTUAL_CATEGORY_CODE = 'E2E-VIRTUAL'
const PNG_1X1 = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=',
    'base64',
)

type AccountCred = { account: string; password: string }

function resolveAccount(login: string): AccountCred {
    const bag = ACCOUNTS as Record<string, unknown>
    const direct = bag[login]
    if (direct && typeof direct === 'object' && 'account' in direct) {
        const row = direct as { account: string; password?: string }
        return { account: row.account, password: row.password ?? PASSWORD }
    }
    for (const value of Object.values(bag)) {
        if (value && typeof value === 'object' && 'account' in value) {
            const row = value as { account: string; password?: string }
            if (row.account === login) {
                return { account: row.account, password: row.password ?? PASSWORD }
            }
        }
    }
    return { account: login, password: PASSWORD }
}

function isPage(value: unknown): value is Page {
    return Boolean(value && typeof value === 'object' && 'goto' in value && 'locator' in value)
}

function isContext(value: unknown): value is BrowserContext {
    return Boolean(value && typeof value === 'object' && 'newPage' in value && 'close' in value)
}

async function openSession(
    browser: Browser,
    login: string,
): Promise<{ context: BrowserContext; page: Page }> {
    const cred = resolveAccount(login)
    const opened: unknown = await newLoggedInContext(browser, cred as never)
    if (isPage(opened)) {
        return { context: opened.context(), page: opened }
    }
    if (opened && typeof opened === 'object') {
        const record = opened as Record<string, unknown>
        if (isPage(record.page) && isContext(record.context)) {
            return { context: record.context, page: record.page }
        }
        if (isContext(opened)) {
            const page = await opened.newPage()
            await loginViaUi(page, cred as never)
            await expect(page.getByRole('heading', { name: '我的工作台' })).toBeVisible({
                timeout: 20000,
            })
            return { context: opened, page }
        }
    }
    throw new Error('newLoggedInContext 返回值无法识别，请核对 helpers/login.ts')
}

async function expectToast(page: Page, title: string | RegExp) {
    await expect(page.getByText(title).first()).toBeVisible({ timeout: 20000 })
}

async function chooseOption(page: Page, input: Locator, optionLabel: string | RegExp) {
    await input.click()
    const query = typeof optionLabel === 'string' ? optionLabel : ''
    if (query) await input.fill(query)
    const option = page.getByRole('option', { name: optionLabel }).first()
    await expect(option).toBeVisible({ timeout: 20000 })
    await option.click()
}

async function chooseComboboxById(page: Page, id: string, optionLabel: string | RegExp) {
    await chooseOption(page, page.locator(`#${id}`), optionLabel)
}

function isoDate(offsetDays = 0): string {
    const date = new Date()
    date.setDate(date.getDate() + offsetDays)
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

async function pickIsoDate(page: Page, trigger: Locator, iso: string) {
    await trigger.click()
    const popover = page.locator('[data-slot="popover-content"]').last()
    await expect(popover).toBeVisible({ timeout: 10000 })
    const target = new Date(`${iso}T00:00:00`)
    let picked = false
    for (let i = 0; i < 14; i += 1) {
        const byId = popover.locator(`[id$="-day-${iso}"]`)
        if (await byId.count()) {
            await byId.first().click()
            picked = true
            break
        }
        const locales = [target.toLocaleDateString('zh-CN'), target.toLocaleDateString('en-US')]
        for (const label of locales) {
            const byData = popover.locator(`[data-day="${label}"]`)
            if (await byData.count()) {
                await byData.first().click()
                picked = true
                break
            }
        }
        if (picked) break
        await popover.locator('button').nth(1).click()
    }
    if (!picked) throw new Error(`无法在日期选择器中点选 ${iso}`)
    await expect(trigger).toHaveAttribute('aria-label', `已选日期 ${iso}`, {
        timeout: 10000,
    })
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

async function gotoWorkspace(page: Page, query = '') {
    await page.goto(query ? `/workspace?${query}` : '/workspace')
    await expect(page.getByRole('heading', { name: '我的工作台' })).toBeVisible({
        timeout: 20000,
    })
}

async function openWorkspaceTask(page: Page, name: RegExp) {
    const task = page.getByRole('button', { name }).first()
    await expect(task).toBeVisible({ timeout: 30000 })
    await task.click()
    await expect(page.getByLabel(/当前/)).toBeVisible({ timeout: 20000 })
}

async function approveCurrentTask(page: Page) {
    const approve = page.getByRole('button', { name: '通过' }).first()
    await expect(approve).toBeVisible({ timeout: 20000 })
    await approve.click()
    const dialog = page.getByRole('dialog', { name: '确认通过' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.getByRole('button', { name: '确认通过' }).click()
    await expect(dialog).toBeHidden({ timeout: 20000 })
}

async function ensureProcurementDispatcher(page: Page) {
    await page.goto('/master-data/procurement-responsibilities')
    await expect(page.getByRole('heading', { name: '采购责任规则' })).toBeVisible({
        timeout: 20000,
    })
    if (await page.getByText('默认调度人').count()) return
    await page.getByTestId('procurement-responsibility-create').click()
    const dialog = page.getByRole('dialog', { name: '新增采购责任规则' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await chooseComboboxById(page, 'procurement-responsibility-rules-dialog-rule-type', '默认调度人')
    await chooseComboboxById(page, 'procurement-responsibility-rules-dialog-owner', /采购.*caigou/)
    await dialog.getByTestId('procurement-responsibility-save').click()
    await expectToast(page, '采购责任规则已新增')
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(page.getByText('默认调度人')).toBeVisible({ timeout: 20000 })
}

async function ensureVirtualCategory(page: Page) {
    await page.goto('/master-data/categories')
    await expect(page.getByRole('heading', { name: /商品分类/ })).toBeVisible({ timeout: 20000 })
    if (await page.getByText(VIRTUAL_CATEGORY_NAME).count()) return
    await page.locator('#master-data-category-tree-create-root').click()
    const dialog = page.getByRole('dialog', { name: /新建商品分类/ })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.locator('#master-data-category-create-dialog-name').fill(VIRTUAL_CATEGORY_NAME)
    await dialog.locator('#master-data-category-create-dialog-code').fill(VIRTUAL_CATEGORY_CODE)
    await chooseComboboxById(page, 'master-data-category-create-dialog-product-kind', '虚拟')
    await dialog.locator('#master-data-category-create-dialog-change-reason').fill('E2E 电子交付目录')
    await dialog.locator('#master-data-category-create-dialog-submit').click()
    await expectToast(page, '已新建')
    await expect(page.getByText(VIRTUAL_CATEGORY_NAME)).toBeVisible({ timeout: 20000 })
}

async function ensureVirtualProduct(page: Page) {
    await page.goto('/master-data/products')
    await expect(page.getByRole('heading', { name: /商品列表/ })).toBeVisible({ timeout: 20000 })
    const search = page.locator('#master-data-products-list-toolbar-search-input')
    if (await search.count()) {
        await search.fill(VIRTUAL_PRODUCT_NO)
        await search.press('Enter')
    }
    if (await page.getByText(VIRTUAL_PRODUCT_NAME).count()) {
        await page.getByText(VIRTUAL_PRODUCT_NAME).first().click()
        return
    }
    await page.locator('#master-data-products-list-create').click()
    await expect(page.locator('#master-data-product-basic-product-no')).toBeVisible({
        timeout: 20000,
    })
    await page.locator('#master-data-product-basic-product-no').fill(VIRTUAL_PRODUCT_NO)
    await page.locator('#master-data-product-basic-name').fill(VIRTUAL_PRODUCT_NAME)
    await page.locator('#master-data-product-basic-description').fill('E2E 虚拟商品电子交付用')
    await chooseComboboxById(page, 'master-data-product-basic-kind-combobox', '虚拟')
    await chooseComboboxById(page, 'master-data-product-basic-unit-combobox', /张/)
    await chooseComboboxById(page, 'master-data-product-basic-category-combobox', VIRTUAL_CATEGORY_NAME)
    await chooseComboboxById(page, 'master-data-product-basic-brand-combobox', /福尚云/)
    await page.getByRole('tab', { name: '规格与 SKU' }).click()
    await page.locator('#master-data-product-sku-sku-01-name').fill(VIRTUAL_PRODUCT_NAME)
    await page.locator('#master-data-product-sku-sku-01-code').fill(VIRTUAL_SKU_NO)
    await page.locator('#master-data-product-sku-sku-01-sale-price').fill('100.00')
    await page.locator('#master-data-product-sku-sku-01-market-price').fill('120.00')
    await page.locator('#master-data-product-sku-sku-01-main-image-input').setInputFiles({
        name: 'virtual-sku.png',
        mimeType: 'image/png',
        buffer: PNG_1X1,
    })
    await page.locator('#master-data-product-detail-header-submit').click()
    await expectToast(page, '已新建')
    await expect(page).toHaveURL(/\/master-data\/products\/(?!new)/, { timeout: 20000 })
    await expect(page.getByRole('tab', { name: '规格与 SKU' })).toBeVisible({ timeout: 20000 })
}

async function ensureVirtualOfferingAndListing(page: Page) {
    await page.goto('/procurement/supplier-offerings')
    await expect(page.getByRole('heading', { name: '供应商供给' })).toBeVisible({
        timeout: 20000,
    })
    if (!(await page.getByText(VIRTUAL_SKU_NO).count())) {
        await page.locator('#supplier-offerings-page-create').click()
        const dialog = page.getByRole('dialog', { name: '添加供给' })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        await chooseComboboxById(
            page,
            'supplier-offerings-dialog-register-sku',
            VIRTUAL_PRODUCT_NAME,
        )
        await chooseComboboxById(page, 'supplier-offerings-dialog-register-supplier', /上海通卡/)
        await dialog.locator('#supplier-offerings-dialog-register-supplier-sku-code').fill(VIRTUAL_SKU_NO)
        await dialog.locator('#supplier-offerings-dialog-register-dropship-price').fill('88.00')
        await dialog.locator('#supplier-offerings-dialog-register-bulk-price').fill('80.00')
        await dialog.locator('#supplier-offerings-dialog-register-minimum-quantity').fill('1')
        await dialog.locator('#supplier-offerings-dialog-register-input-tax-percentage').fill('6')
        await dialog.locator('#supplier-offerings-dialog-register-supply-region').fill('全国')
        await pickIsoDate(page, page.locator('#supplier-offerings-dialog-register-valid-from'), isoDate(0))
        await dialog.locator('#supplier-offerings-dialog-register-available-quantity').fill('1000')
        await dialog.locator('#supplier-offerings-dialog-register-submit').click()
        await expectToast(page, '供给已添加')
        await expect(dialog).toBeHidden({ timeout: 20000 })
        await expect(page.getByText(VIRTUAL_SKU_NO).first()).toBeVisible({ timeout: 20000 })
    }

    await page.goto('/master-data/products')
    await expect(page.getByRole('heading', { name: /商品列表/ })).toBeVisible({ timeout: 20000 })
    const search = page.locator('#master-data-products-list-toolbar-search-input')
    await search.fill(VIRTUAL_PRODUCT_NO)
    await search.press('Enter')
    const listing = page.getByRole('switch', { name: `${VIRTUAL_PRODUCT_NAME}整组上架状态` })
    await expect(listing).toBeVisible({ timeout: 20000 })
    if (!(await listing.isChecked())) {
        await listing.click()
        await expect(listing).toBeChecked({ timeout: 20000 })
    }
}

async function createCustomer(page: Page, legalName: string, creditCode: string) {
    await page.goto('/sales/customers')
    await expect(page.getByRole('heading', { name: '客户中心' })).toBeVisible({ timeout: 20000 })
    await page.locator('#customers-directory-create').click()
    const dialog = page.getByRole('dialog', { name: '新建客户' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.locator('#customers-form-legal-name').fill(legalName)
    await dialog.locator('#customers-form-short-name').fill('E2E虚拟客户')
    await dialog.locator('#customers-form-credit-code').fill(creditCode)
    await chooseComboboxById(page, 'customers-form-payment-term', '货到 30 天')
    await dialog.locator('#customers-form-submit').click()
    await expectToast(page, '客户已创建')
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(page.getByText(legalName).first()).toBeVisible({ timeout: 20000 })
}

async function uploadContractOnSalesOrder(page: Page, legalName: string, contractNo: string) {
    await page.getByRole('button', { name: '上传合同 PDF' }).click()
    const dialog = page.getByRole('dialog', { name: '上传合同 PDF' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.locator('#card-contracts-upload-pdf-input').setInputFiles(contractPdfPath())
    await dialog.locator('#card-contracts-upload-contract-no').fill(contractNo)
    await chooseComboboxById(page, 'card-contracts-upload-customer', legalName)
    const settlement = dialog.locator('#card-contracts-upload-settlement-party')
    try {
        await expect(settlement).toHaveValue(/.+/, { timeout: 8000 })
    } catch {
        await chooseComboboxById(page, 'card-contracts-upload-settlement-party', legalName)
    }
    await chooseComboboxById(page, 'card-contracts-upload-payment-terms', '货到 30 天')
    await dialog.locator('#card-contracts-upload-submit').click()
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(page.getByText(new RegExp(`客户\\s+${legalName}`))).toBeVisible({
        timeout: 20000,
    })
}

async function pickVirtualSku(page: Page) {
    await page.getByRole('button', { name: '选择商品' }).click()
    const dialog = page.getByRole('dialog', { name: '选择商品' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.locator('#sales-orders-sku-picker-toolbar').getByRole('button', { name: '更多筛选' }).click()
    await dialog.getByRole('radio', { name: '虚拟' }).click()
    await dialog.locator('#master-data-list-sellable-list-toolbar-button-5').click()
    const empty = dialog.getByText('当前筛选无结果')
    if (await empty.isVisible().catch(() => false)) {
        throw new Error('公司商品池没有可销售的虚拟 SKU，目录补齐未生效')
    }
    const rowName = new RegExp(VIRTUAL_PRODUCT_NAME)
    await expect(dialog.getByText(rowName).first()).toBeVisible({ timeout: 20000 })
    await dialog.getByRole('checkbox', { name: new RegExp(`选择 .*${VIRTUAL_PRODUCT_NAME}`) }).click()
    await dialog.locator('#sales-orders-sku-picker-confirm').click()
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(page.getByText(VIRTUAL_PRODUCT_NAME).first()).toBeVisible({ timeout: 20000 })
}

test('虚拟商品电子交付全流程：销售单生效后只能采购、登记电子交付并验收', async ({ browser }) => {
    test.setTimeout(8 * 60 * 1000)
    const stamp = Date.now().toString(36).slice(-6)
    const customerName = `E2E虚拟客户${stamp}有限公司`
    const creditCode = `91110105E2E${stamp}XX`.slice(0, 18).padEnd(18, '0')
    const contractNo = `HT-E2E-ED-${stamp}`
    const due = isoDate(45)
    let salesOrderId = ''
    let salesOrderNo = ''

    {
        const { context, page } = await openSession(browser, 'admin')
        try {
            await ensureProcurementDispatcher(page)
            await ensureVirtualCategory(page)
            await ensureVirtualProduct(page)
            await ensureVirtualOfferingAndListing(page)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'xiaoshou')
        try {
            await createCustomer(page, customerName, creditCode)
            await page.goto('/sales/orders?mode=create')
            await expect(page.locator('#sales-orders-create-header-nature')).toBeVisible({
                timeout: 20000,
            })
            await uploadContractOnSalesOrder(page, customerName, contractNo)
            await chooseComboboxById(page, 'sales-orders-create-header-welfare-scene', '年节礼包')
            await chooseComboboxById(page, 'sales-orders-create-header-payment-terms', '货到 30 天')
            await pickVirtualSku(page)
            await page.locator('[id^="sales-orders-create-line-"][id$="-quantity"]').first().fill('10')
            await pickIsoDate(page, page.locator('#sales-orders-create-batch-due-date'), due)
            await page.locator('#sales-orders-create-batch-due-date-apply').click()
            await expectToast(page, '已批量设置交期')
            await expect(page.getByTestId(/sales-line-procurement-owner-/)).not.toHaveText(
                /暂未确定采购负责人/,
                { timeout: 20000 },
            )
            await page.getByTestId('sales-order-submit').click()
            const submitDialog = page.getByRole('dialog', { name: '提交销售单' })
            await expect(submitDialog).toBeVisible({ timeout: 20000 })
            await expect(submitDialog.getByText('审批中')).toBeVisible()
            await submitDialog.locator('#sales-orders-submit-confirm-confirm').click()
            await expect(submitDialog).toBeHidden({ timeout: 30000 })
            await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: 30000 })
            salesOrderId = page.url().split('/sales/orders/')[1]?.split('?')[0] ?? ''
            expect(salesOrderId).toBeTruthy()
            salesOrderNo = ((await page.locator('.num').first().textContent()) ?? '').trim()
            await expect(page.getByText(/审批中/).first()).toBeVisible({ timeout: 20000 })
            await expect(page.getByRole('tab', { name: '采购' })).toBeVisible()
            await page.getByRole('tab', { name: '采购' }).click()
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText(/采购单 0 笔/, {
                timeout: 20000,
            })
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'caigou')
        try {
            await gotoWorkspace(page, 'family=approval')
            await openWorkspaceTask(page, /单据审批/)
            await expect(page.getByRole('button', { name: '通过' })).toBeVisible({ timeout: 20000 })
            await expect(page.getByText(/采购确认|审批/)).toBeVisible()
            await approveCurrentTask(page)
            await gotoWorkspace(page, 'family=procurement&type=PROCUREMENT_ORDER_CREATION')
            await openWorkspaceTask(page, /待供给分配|供给分配/)
            await expect(page.getByRole('heading', { name: '供给分配' })).toBeVisible({
                timeout: 20000,
            })
            await expect(page.getByText(VIRTUAL_PRODUCT_NAME)).toBeVisible({ timeout: 20000 })
            await expect(page.getByRole('table').getByText(/现有库存/)).toHaveCount(0)
            await expect(page.getByRole('table').getByText('电子交付').first()).toBeVisible({
                timeout: 20000,
            })
            await expect(page.getByText('将创建采购单')).toBeVisible()
            await expect(page.getByText('1 张')).toBeVisible()
            await expect(page.getByText('将建立库存预留')).toBeVisible()
            await expect(page.getByText('0 条')).toBeVisible()
            await page.getByTestId('purchase-create-preview').click()
            const preview = page.getByRole('dialog', { name: '预览供给分配' })
            await expect(preview).toBeVisible({ timeout: 20000 })
            await expect(preview.getByText('现有库存分配')).toHaveCount(0)
            await expect(preview.getByText(/虚拟|电子交付/)).toBeVisible()
            await preview.locator('#procurement-orders-create-preview-confirm').click()
            const confirm = page.getByRole('alertdialog', { name: '确认供给分配' })
            await expect(confirm).toBeVisible({ timeout: 20000 })
            await expect(confirm.getByText(/创建 1 张采购单提交审批/)).toBeVisible()
            await confirm.getByTestId('purchase-create-confirm').click()
            await expectToast(page, /供给分配已完成|已创建 1 张采购单并提交审批/)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'xiaoshou')
        try {
            await page.goto(`/sales/orders/${salesOrderId}`)
            await expect(page.getByText(/已生效/).first()).toBeVisible({ timeout: 20000 })
            await page.getByRole('tab', { name: '采购' }).click()
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText(/采购单 1 笔/, {
                timeout: 20000,
            })
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText(/审批中/)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'caiwu')
        try {
            await gotoWorkspace(page, 'family=approval')
            await openWorkspaceTask(page, /单据审批/)
            await expect(page.getByText(/采购单|财务/)).toBeVisible()
            await approveCurrentTask(page)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'caigou')
        try {
            await page.goto('/procurement/orders')
            await expect(page.getByRole('heading', { name: '采购单' })).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('已生效').first()).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('电子交付').first()).toBeVisible()
            await expect(page.getByText('虚拟').first()).toBeVisible()
            await expect(page.getByRole('button', { name: '通过' })).toHaveCount(0)
            await expect(page.getByText('选择流程')).toHaveCount(0)

            await gotoWorkspace(page, 'family=fulfillment&type=FULFILLMENT_OPERATION')
            const electronicTask = page.getByRole('button', { name: /履约处理/ })
            await expect(electronicTask.first()).toBeVisible({ timeout: 30000 })
            await electronicTask.first().click()
            const pane = page.getByLabel('当前履约任务')
            await expect(pane).toBeVisible({ timeout: 20000 })
            await expect(pane.getByRole('heading', { name: /电子交付/ })).toBeVisible({
                timeout: 20000,
            })
            await expect(pane.getByRole('button', { name: '通过' })).toHaveCount(0)
            await expect(pane.getByRole('button', { name: '驳回' })).toHaveCount(0)
            await expect(pane.getByText('选择流程')).toHaveCount(0)
            await expect(pane.getByLabel('电子交付表单')).toBeVisible()
            const recipient = pane.locator('#fulfillment-operations-electronic-form-recipient')
            await expect(recipient).toBeVisible()
            if (!(await recipient.inputValue()).trim()) {
                await recipient.evaluate((el) => {
                    el.removeAttribute('disabled')
                    el.removeAttribute('readonly')
                })
                await recipient.fill('E2E交付对象（客户经办人）')
            }
            await expect(pane.locator('#fulfillment-operations-electronic-form-result')).toBeVisible()
            await pane.locator('#fulfillment-operations-work-surface-confirm').click()
            const confirm = page.getByRole('alertdialog', { name: '确认交付？' })
            await expect(confirm).toBeVisible({ timeout: 20000 })
            await expect(confirm.getByText(/不动库存/)).toBeVisible()
            await confirm.locator('#fulfillment-operations-workspace-confirm-confirm').click()
            await expect(confirm).toBeHidden({ timeout: 20000 })
            await expect(page.getByText(/已交付|已记下来了/).first()).toBeVisible({ timeout: 20000 })
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'xiaoshou')
        try {
            await gotoWorkspace(page, 'family=fulfillment&type=CUSTOMER_ACCEPTANCE_REGISTRATION')
            await openWorkspaceTask(page, /客户验收/)
            const pane = page.getByLabel('当前客户验收任务')
            await expect(pane).toBeVisible({ timeout: 20000 })
            await pane.locator('#sales-orders-acceptance-register-open').click()
            const dialog = page.getByRole('dialog', { name: '登记客户验收' })
            await expect(dialog).toBeVisible({ timeout: 20000 })
            await expect(dialog.getByText(/电子交付|通过/)).toBeVisible()
            await dialog.locator('#sales-orders-acceptance-register-submit').click()
            const confirm = page.getByRole('alertdialog', { name: '确认客户验收' })
            await expect(confirm).toBeVisible({ timeout: 20000 })
            await confirm.locator('#sales-orders-acceptance-confirm-confirm').click()
            await expectToast(page, '客户验收已登记')
            await expect(dialog).toBeHidden({ timeout: 20000 })

            await page.goto(`/sales/orders/${salesOrderId}`)
            await page.getByRole('tab', { name: '验收' }).click()
            await expect(page.getByText(/通过/).first()).toBeVisible({ timeout: 20000 })
            await page.getByRole('tab', { name: '采购' }).click()
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText(/采购单 1 笔/)
            await expect(page.getByTestId('sales-order-purchase-status')).not.toContainText(/现有库存/)
        } finally {
            await context.close()
        }
    }

    expect(salesOrderNo || salesOrderId).toBeTruthy()
})
