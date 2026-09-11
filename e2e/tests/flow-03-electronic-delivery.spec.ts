/**
 * 流程: [flow-03] 虚拟商品电子交付
 * 文档: docs/erp-phase-1.md §7.3.3 + §7.4（供给分配）+ §6.2 电子交付记录
 * 账号: admin（目录补齐）→ xiaoshou（客户/合同/销售单/验收）
 *       → caigou（采购确认、供给分配、电子交付）→ caiwu（采购单审批）
 *
 * 验收：采购生效生成电子交付任务；采购提交对象、实际时间、结果与图片凭证；
 *       销售登记客户验收。交付不走审批，不影响自有库存。
 * 目录种子无 VIRTUAL SKU 时，通过 UI 补齐本流程使用的商品。
 */
import { test, expect, type Browser, type BrowserContext, type Locator, type Page } from '@playwright/test'
import fs from 'node:fs'
import path from 'node:path'

import { payOnlySupplierTask } from '../helpers/payments'
import { ACCOUNTS } from '../helpers/accounts'
import { loginViaUi, newLoggedInContext } from '../helpers/login'
import { openFulfillmentWorkspaceForm, readHeaderDocumentNumber } from "../helpers/ui"

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

async function dismissToasts(page: Page) {
    // 悬浮提示会遮挡对话框按钮且悬停时暂停自动消失，操作前全部关闭。
    for (let i = 0; i < 5; i += 1) {
        const dismiss = page
            .locator('[data-slot="toast"]')
            .getByRole("button", { name: "关闭提示", includeHidden: true })
            .first()
        if ((await dismiss.count()) === 0) break
        await dismiss.click({ timeout: 5_000 }).catch(() => undefined)
    }
}

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

async function expectToast(page: Page, title: string | RegExp) {
    await expect(page.getByText(title).first()).toBeVisible({ timeout: 20000 })
    // 关闭已确认的悬浮提示，避免其遮挡后续按钮造成偶发点击失败。
    await dismissToasts(page)
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
    await expect(page.getByRole('region', { name: '当前工作台任务' })).toBeVisible({ timeout: 20000 })
}

async function approveCurrentTask(page: Page) {
    const approve = page.getByRole('button', { name: /^(通过|同意审批)$/ }).first()
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
    // 规则列表在标题之后加载，先等列表接口返回再判断是否已存在，避免重复创建触发 409。
    await page
        .waitForResponse(
            (response) =>
                response.request().method() === 'GET' &&
                response.url().includes('procurement-responsibility-rules'),
            { timeout: 20000 },
        )
        .catch(() => undefined)
    // count() 不等待渲染，数据到达后轮询等待行渲染，避免误判为不存在。
    const dispatcher = page.getByText('默认调度人')
    await expect(dispatcher.first()).toBeVisible({ timeout: 8000 }).catch(() => undefined)
    if (await dispatcher.count()) return
    await page.getByTestId('procurement-responsibility-create').click()
    const dialog = page.getByRole('dialog', { name: '新增采购责任规则' })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await chooseComboboxById(page, 'procurement-responsibility-rules-dialog-rule-type', '默认调度人')
    await chooseComboboxById(page, 'procurement-responsibility-rules-dialog-owner', /采购.*caigou/)
    await dialog.getByTestId('procurement-responsibility-save').click()
    // 规则已存在时保存返回 409，对话框保持打开：此时直接关闭复用已有规则。
    const saved = page.getByText(/采购责任规则已新增|采购责任规则已更新/).first()
    await expect(saved).toBeVisible({ timeout: 10000 }).catch(() => undefined)
    if (await saved.count()) {
        await expectToast(page, /采购责任规则已新增|采购责任规则已更新/)
    } else if (await dialog.isVisible().catch(() => false)) {
        await dialog.getByRole('button', { name: '取消' }).click()
    }
    await expect(dialog).toBeHidden({ timeout: 20000 })
    await expect(page.getByText('默认调度人')).toBeVisible({ timeout: 20000 })
}

async function ensureVirtualCategory(page: Page) {
    await page.goto('/master-data/categories')
    await expect(page.getByRole('heading', { name: '商品分类', exact: true })).toBeVisible({ timeout: 20000 })
    // 分类树在标题之后加载，先等列表接口返回再判断，避免重复创建触发 409。
    await page
        .waitForResponse(
            (response) =>
                response.request().method() === 'GET' &&
                response.url().includes('product-categories'),
            { timeout: 20000 },
        )
        .catch(() => undefined)
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
    await expect(page.getByRole('heading', { name: '商品列表', exact: true, level: 1 })).toBeVisible({ timeout: 20000 })
    const search = page.locator('#master-data-products-list-toolbar-search-input')
    if (await search.count()) {
        await search.fill(VIRTUAL_PRODUCT_NO)
        await search.press('Enter')
        await page
            .waitForResponse(
                (response) =>
                    response.request().method() === 'GET' &&
                    response.url().includes('/admin/products'),
                { timeout: 20000 },
            )
            .catch(() => undefined)
    }
    if (await page.getByText(VIRTUAL_PRODUCT_NAME).count()) {
        await page.getByText(VIRTUAL_PRODUCT_NAME).first().click()
        return
    }
    await page.locator('#master-data-products-list-create').click()
    // 详情页各分区使用 detail 前缀 id（basic 分区为 master-data-product-detail-basic）。
    const basicId = 'master-data-product-detail-basic'
    await expect(page.locator(`#${basicId}-product-no`)).toBeVisible({
        timeout: 20000,
    })
    await page.locator(`#${basicId}-product-no`).fill(VIRTUAL_PRODUCT_NO)
    await page.locator(`#${basicId}-name`).fill(VIRTUAL_PRODUCT_NAME)
    await page.locator(`#${basicId}-description`).fill('E2E 虚拟商品电子交付用')
    await chooseComboboxById(page, `${basicId}-kind-combobox`, '虚拟')
    await chooseComboboxById(page, `${basicId}-unit-combobox`, /张/)
    await chooseComboboxById(page, `${basicId}-category-combobox`, VIRTUAL_CATEGORY_NAME)
    await chooseComboboxById(page, `${basicId}-brand-combobox`, /福尚云/)
    // SKU 与商品资料同页编辑，使用稳定分区定位。
    await page.locator('#product-section-sku').scrollIntoViewIfNeeded()
    // SKU 编码/名称/主图改到行内「编辑资料」对话框维护，价格仍在表格行内直接填写。
    await page.getByRole('button', { name: '编辑资料' }).click()
    const skuDialog = page.getByRole('dialog', { name: 'SKU 资料' })
    await expect(skuDialog).toBeVisible({ timeout: 20000 })
    await skuDialog.getByLabel('默认规格 SKU 编码').fill(VIRTUAL_SKU_NO)
    await skuDialog.getByLabel('默认规格 SKU 名称').fill(VIRTUAL_PRODUCT_NAME)
    await skuDialog.getByRole('button', { name: '完成编辑' }).click()
    await expect(skuDialog).toBeHidden({ timeout: 20000 })
    // 主图在表格行内 tile 上传（启用 SKU 必填）：点行内「选择主图」走文件选择器。
    const imageGroup = page.getByRole('group', { name: /主图/ }).first()
    const [chooser] = await Promise.all([
        page.waitForEvent('filechooser', { timeout: 20000 }),
        imageGroup.getByRole('button', { name: '选择主图' }).click(),
    ])
    await chooser.setFiles({
        name: 'virtual-sku.png',
        mimeType: 'image/png',
        buffer: PNG_1X1,
    })
    await page.getByLabel('默认规格 销售价').fill('100.00')
    await page.getByLabel('默认规格 市场价').fill('120.00')
    await page.locator('#master-data-product-detail-header-submit').click()
    // 保存改为两步确认：先在「创建商品」框填写变更原因，再确认保存。
    const saveDialog = page.getByRole('dialog', { name: '创建商品' })
    await expect(saveDialog).toBeVisible({ timeout: 20000 })
    await saveDialog.locator('#master-data-product-detail-effective-reason').fill('E2E 电子交付新建商品')
    await saveDialog.locator('#master-data-product-save-confirm').click()
    // 主数据在重置间保留，商品已存在时后端返回 409，对话框内显示阻断反馈而不关闭。
    await expect(
        saveDialog.getByText('资料已被他人更新').or(page.getByText('已新建').first()),
    ).toBeVisible({ timeout: 30000 })
    if (await saveDialog.getByText('资料已被他人更新').count()) {
        // 数据已存在：关闭保存框，复用已有商品。
        await saveDialog.getByRole('button', { name: '继续编辑' }).click()
        await expect(saveDialog).toBeHidden({ timeout: 20000 })
        return
    }
    await expect(saveDialog).toBeHidden({ timeout: 30000 })
    // 主数据在重置间保留，商品可能已存在：已新建与已被他人更新均为合法结果。
    await expect(
        page.getByText('已新建').first().or(page.getByText('资料已被他人更新')),
    ).toBeVisible({ timeout: 20000 })
    if (await page.getByText('资料已被他人更新').count()) return
    await expectToast(page, '已新建')
    await expect(page).toHaveURL(/\/master-data\/products\/(?!new)/, { timeout: 20000 })
    await expect(page.getByRole('heading', { name: '规格与 SKU', exact: true })).toBeVisible({ timeout: 20000 })
}

async function ensureVirtualOfferingAndListing(page: Page) {
    await page.goto('/procurement/supplier-offerings')
    await expect(page.getByRole('heading', { name: '供应商供给' })).toBeVisible({
        timeout: 20000,
    })
    // 供给列表在标题之后加载，先等列表接口返回再判断，避免重复登记。
    await page
        .waitForResponse(
            (response) =>
                response.request().method() === 'GET' &&
                response.url().includes('supplier-offerings'),
            { timeout: 20000 },
        )
        .catch(() => undefined)
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
        await dialog.locator('#supplier-offerings-dialog-register-supply-region').press('ArrowDown')
        await page.getByRole('option', { name: '全国', exact: true }).click()
        await pickIsoDate(page, page.locator('#supplier-offerings-dialog-register-valid-from'), isoDate(0))
        await dialog.locator('#supplier-offerings-dialog-register-available-quantity').fill('1000')
        await dialog.locator('#supplier-offerings-dialog-register-submit').click()
        // 主数据在重置间保留，供给可能已登记：成功与重复均为合法结果。
        await expect(
            page.getByText('供给已添加').first().or(page.getByText('已登记供给')),
        ).toBeVisible({ timeout: 20000 })
        if (await page.getByText('已登记供给').count()) {
            // 409 本身证明供给已存在，直接复用，不依赖列表分页可见性。
            await dialog.getByRole('button', { name: '关闭' }).first().click()
            await page.getByRole('alertdialog').getByRole('button', { name: '放弃更改' }).click()
            await expect(dialog).toBeHidden({ timeout: 20000 })
        } else {
            await expectToast(page, '供给已添加')
            await expect(dialog).toBeHidden({ timeout: 20000 })
            await expect(page.getByText(VIRTUAL_SKU_NO).first()).toBeVisible({ timeout: 20000 })
        }
    }

    await page.goto('/master-data/products')
    await expect(page.getByRole('heading', { name: '商品列表', exact: true, level: 1 })).toBeVisible({ timeout: 20000 })
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
    // 列表行链接展示客户简称，非法定全称。
    await expect(page.getByRole('link', { name: 'E2E虚拟客户' })).toBeVisible({ timeout: 20000 })
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
    await page.locator('[id^="sales-orders-create-line-"][id$="-pick-sku"]').click()
    const dialog = page.getByRole('dialog', { name: '更换销售商品' })
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
            await clickWithoutToastOverlay(page, submitDialog.locator('#sales-orders-submit-confirm-confirm'))
            await expect(submitDialog).toBeHidden({ timeout: 30000 })
            await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: 30000 })
            salesOrderId = page.url().split('/sales/orders/')[1]?.split('?')[0] ?? ''
            expect(salesOrderId).toBeTruthy()
            salesOrderNo = await readHeaderDocumentNumber(page)
            await expect(page.getByText(/审批中/).first()).toBeVisible({ timeout: 20000 })
            await expect(page.getByRole('tab', { name: '采购' })).toBeVisible()
            await page.getByRole('tab', { name: '采购' }).click()
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText('待采购', {
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
            await openWorkspaceTask(page, /销售单审批/)
            await expect(page.getByRole('button', { name: /^(通过|同意审批)$/ })).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('采购确认').first()).toBeVisible({ timeout: 20000 })
            await approveCurrentTask(page)
            await gotoWorkspace(page, 'family=procurement&type=PROCUREMENT_ORDER_CREATION')
            await openWorkspaceTask(page, /待供给分配|供给分配/)
            await expect(page.getByRole('heading', { name: '供给分配' })).toBeVisible({
                timeout: 20000,
            })
            await expect(page.getByText(VIRTUAL_PRODUCT_NAME).first()).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('现有库存').filter({ visible: true })).toHaveCount(0)
            await expect(page.getByText('电子交付').filter({ visible: true }).first()).toBeVisible({
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
            await expect(preview.getByText('电子交付').first()).toBeVisible({ timeout: 20000 })
            await preview.locator('#procurement-orders-create-preview-confirm').click()
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
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText('采购已覆盖', {
                timeout: 20000,
            })
            // 销售账号无采购单明细查看权限，面板仅显示计数提示。
            await expect(page.getByTestId('sales-order-purchase-count-only')).toContainText(
                /已创建 1 张采购单/,
                { timeout: 20000 },
            )
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'caiwu')
        try {
            await gotoWorkspace(page, 'family=approval')
            await openWorkspaceTask(page, /采购单审批/)
            await expect(
                page.getByRole('heading', { name: /采购单/ }).or(page.getByText(/PO-/)).first(),
            ).toBeVisible({
                timeout: 20000,
            })
            await approveCurrentTask(page)
        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'fukuan')
        try {
            await payOnlySupplierTask(page)
        } finally { await context.close() }
    }

    {
        const { context, page } = await openSession(browser, 'caigou')
        try {
            await page.goto('/procurement/orders')
            await expect(page.getByRole('heading', { name: '采购单', exact: true })).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('已生效').first()).toBeVisible({ timeout: 20000 })
            // 采购列表必须展示单据真实履约责任，虚拟商品不得误标为入仓。
            await expect(page.getByRole('table').getByText('虚拟 / 电子交付', { exact: true })).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('虚拟').first()).toBeVisible()
            await expect(page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)
            await expect(page.getByText('选择流程')).toHaveCount(0)

            await gotoWorkspace(page, 'family=fulfillment&type=FULFILLMENT_OPERATION')
            await openWorkspaceTask(page, /履约处理/)
            await openFulfillmentWorkspaceForm(page)
            await expect(page.locator('[aria-label="电子交付表单"]')).toBeVisible({ timeout: 30000 })
            await expect(page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)
            await page.locator('#fulfillment-operations-electronic-form-recipient').fill('E2E 客户企业邮箱收件人')
            await page.locator('#fulfillment-operations-electronic-form-evidence-input').setInputFiles({ name: 'electronic-delivery.png', mimeType: 'image/png', buffer: PNG_1X1 })
            await chooseComboboxById(page, 'fulfillment-operations-electronic-form-result', '成功')
            const quantity = await page.getByLabel('交付数量', { exact: true }).inputValue()
            expect(Number(quantity)).toBeGreaterThan(0)
            await page.locator('#fulfillment-operations-work-surface-confirm').click()
            const confirm = page.getByRole('alertdialog', { name: '确认交付？' })
            await expect(confirm).toBeVisible({ timeout: 20000 })
            const responsePromise = page.waitForResponse(response => response.request().method() === 'POST' && /\/admin\/electronic-deliveries\/[^/]+\/confirm$/.test(response.url()))
            await confirm.getByRole('button', { name: '确认交付', exact: true }).click()
            const response = await responsePromise
            const body = await response.json()
            expect(response.ok(), JSON.stringify(body)).toBeTruthy()
            expect(body.data).toMatchObject({ status: 'CONFIRMED', result: 'SUCCESS', quantity })
            await expect(confirm).toBeHidden({ timeout: 20000 })

        } finally {
            await context.close()
        }
    }

    {
        const { context, page } = await openSession(browser, 'xiaoshou')
        try {
            await gotoWorkspace(page, 'family=fulfillment&type=CUSTOMER_ACCEPTANCE_REGISTRATION')
            await openWorkspaceTask(page, /客户验收登记/)
            await page.locator('#sales-orders-acceptance-register-open').click()
            await expect(page.getByRole('dialog', { name: '登记客户验收' })).toBeVisible({ timeout: 20000 })
            await expect(page.getByText('电子交付').first()).toBeVisible()
            await page.locator('#sales-orders-acceptance-register-submit').click()
            await expect(page.getByRole('heading', { name: '确认客户验收' })).toBeVisible({ timeout: 20000 })
            await page.locator('#sales-orders-acceptance-confirm-confirm').click()
            await expectToast(page, '客户验收已登记')
            await expect(page.getByRole('dialog', { name: '登记客户验收' })).toBeHidden({ timeout: 20000 })

            await page.goto(`/sales/orders/${salesOrderId}`)
            await page.getByRole('tab', { name: '验收', exact: true }).click()
            await expect(page.getByText(/已通过.*已交付/)).toBeVisible({ timeout: 20000 })
            await expect(page.locator('#sales-orders-acceptance-register-open')).toHaveCount(0)
            await page.getByRole('tab', { name: '采购' }).click()
            await expect(page.getByTestId('sales-order-purchase-status')).toContainText('采购已覆盖')
            await expect(page.getByTestId('sales-order-purchase-status')).not.toContainText(/现有库存/)
        } finally {
            await context.close()
        }
    }

    expect(salesOrderNo || salesOrderId).toBeTruthy()
})
