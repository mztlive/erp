/**
 * [flow-04] 线下服务履约
 *
 * 文档：docs/erp-phase-1.md §7.3.3 + §7.4（供给分配为唯一选源；线下服务不得分配现有库存）
 * 账号：xiaoshou（销售） / caigou（采购确认、供给分配、服务履约） / caiwu（采购单审批）
 *        admin 仅在采购责任规则缺失时补默认调度人（主数据，非业务单据）
 *
 * 文档-代码差异（以代码为准）：
 * - 服务履约表单没有独立「服务对象」字段，而是完成数量 + 履约结果 + 服务时间 + 服务地点 + 图片凭证 + 完成说明
 * - 供给分配页文案仍写「优先推荐现有库存」，服务 SKU 的推荐结果只有采购/线下服务
 * - 客户验收成功 toast 描述使用「已过账」（与 ui-glossary 禁用「过账」不一致）
 * - ServiceFulfillment / CustomerAcceptance 为 NO_APPROVAL，工作台原地确认，不出现审批决定栏
 */
import { existsSync } from 'node:fs'
import path from 'node:path'
import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test'

import { ACCOUNTS } from '../helpers/accounts'
import { loginViaUi, newLoggedInContext } from '../helpers/login'

const PASSWORD = '123456'
const TIMEOUT = 20_000
const SERVICE_SKU_NO = 'SVC-INSTALL-01'
const SERVICE_SKU_NAME = '家电上门安装'
const PNG_1X1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
)
const MINIMAL_PDF = Buffer.from(
  `%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj
xref
0 4
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
trailer<</Size 4/Root 1 0 R>>
startxref
190
%%EOF`,
)

type Session = { context: BrowserContext; page: Page }

function isoDate(offsetDays = 0): string {
  const date = new Date()
  date.setDate(date.getDate() + offsetDays)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function uniqueCreditCode(): string {
  const stamp = Date.now().toString().slice(-10)
  return `91110108MA${stamp}`
}

function accountOf(role: 'sales' | 'procurement' | 'finance' | 'admin'): {
  account: string
  password: string
} {
  const bag = ACCOUNTS as Record<string, { account?: string; password?: string } | undefined>
  const fallback: Record<string, string> = {
    sales: 'xiaoshou',
    procurement: 'caigou',
    finance: 'caiwu',
    admin: 'admin',
  }
  return {
    account: bag[role]?.account ?? fallback[role],
    password: bag[role]?.password ?? PASSWORD,
  }
}

async function openSession(browser: Browser, role: 'sales' | 'procurement' | 'finance' | 'admin'): Promise<Session> {
  const cred = accountOf(role)
  const session = (await newLoggedInContext(browser, cred.account)) as Session
  if (session?.page && session?.context) return session
  const context = await browser.newContext()
  const page = await context.newPage()
  await loginViaUi(page, cred.account, cred.password)
  await expect(page.getByRole('heading', { name: '我的工作台' })).toBeVisible({ timeout: TIMEOUT })
  return { context, page }
}

async function gotoHeading(page: Page, href: string, heading: string) {
  await page.goto(href)
  await expect(page.getByRole('heading', { name: heading })).toBeVisible({ timeout: TIMEOUT })
}

async function expectToast(page: Page, title: string | RegExp) {
  await expect(page.getByText(title).first()).toBeVisible({ timeout: TIMEOUT })
}

async function chooseOption(page: Page, label: string | RegExp, option: string | RegExp) {
  const field = page.getByLabel(label).first()
  await field.click()
  const item = page
    .getByRole('option', { name: option })
    .or(page.locator('[data-slot="combobox-item"]').filter({ hasText: option }))
    .first()
  await expect(item).toBeVisible({ timeout: TIMEOUT })
  await item.click()
}

async function pickCalendarIso(page: Page, iso: string) {
  const day = page.locator(`button[id$="-day-${iso}"]:not([disabled])`).first()
  await expect(day).toBeVisible({ timeout: TIMEOUT })
  await day.click()
}

async function pickLabeledDate(page: Page, label: string | RegExp, iso: string) {
  await page.getByLabel(label).first().click()
  await pickCalendarIso(page, iso)
}

async function searchRemoteCombobox(page: Page, label: string | RegExp, query: string, option: string | RegExp) {
  const field = page.getByLabel(label).first()
  await field.click()
  await field.fill(query)
  const item = page
    .getByRole('option', { name: option })
    .or(page.locator('[data-slot="combobox-item"]').filter({ hasText: option }))
    .first()
  await expect(item).toBeVisible({ timeout: TIMEOUT })
  await item.click()
}

async function contractPdf(): Promise<string | { name: string; mimeType: string; buffer: Buffer }> {
  const fixture = path.join(process.cwd(), 'fixtures', 'sample-contract.pdf')
  if (existsSync(fixture)) return fixture
  return { name: 'sample-contract.pdf', mimeType: 'application/pdf', buffer: MINIMAL_PDF }
}

async function refreshWorkspace(page: Page) {
  await page.goto('/workspace')
  await expect(page.getByRole('heading', { name: '我的工作台' })).toBeVisible({ timeout: TIMEOUT })
  const refresh = page.locator('#workspace-home-refresh')
  if (await refresh.isVisible()) await refresh.click()
}

async function searchWorkspace(page: Page, query: string) {
  const search = page.locator('#workspace-queue-toolbar-search-input')
  await search.fill(query)
  await search.press('Enter')
}

async function openInboxTask(
  page: Page,
  family: 'approval' | 'procurement' | 'fulfillment',
  name: RegExp,
  query?: string,
) {
  await refreshWorkspace(page)
  await page.locator('#workspace-queue-scope-inbox').click()
  await page.locator(`#workspace-family-nav-${family}`).click()
  if (query) await searchWorkspace(page, query)
  const task = page.getByRole('button', { name }).first()
  if ((await task.count()) === 0 && query) {
    const search = page.locator('#workspace-queue-toolbar-search-input')
    await search.fill('')
    await search.press('Enter')
  }
  await expect(page.getByRole('button', { name }).first()).toBeVisible({ timeout: TIMEOUT })
  await page.getByRole('button', { name }).first().click()
}

async function approveCurrentTask(page: Page, nodeHint?: string | RegExp) {
  if (nodeHint) {
    await expect(page.getByText(nodeHint).first()).toBeVisible({ timeout: TIMEOUT })
  }
  await page.getByRole('button', { name: '通过', exact: true }).click()
  await expect(page.getByRole('heading', { name: '确认通过' })).toBeVisible({ timeout: TIMEOUT })
  await page.getByRole('button', { name: '确认通过' }).click()
  await expect(page.getByRole('heading', { name: '确认通过' })).toBeHidden({ timeout: TIMEOUT })
}

async function ensureProcurementDispatcher(page: Page) {
  await gotoHeading(page, '/master-data/procurement-responsibilities', '采购责任规则')
  if ((await page.getByText('默认调度人').count()) > 0) return
  await page.getByRole('button', { name: '新增规则' }).click()
  await expect(page.getByRole('heading', { name: '新增采购责任规则' })).toBeVisible({ timeout: TIMEOUT })
  await chooseOption(page, '规则类型', '默认调度人')
  await chooseOption(page, '采购负责人', /采购 · caigou|caigou/)
  await page.getByRole('button', { name: '保存规则' }).click()
  await expectToast(page, '采购责任规则已新增')
  await expect(page.getByText('默认调度人')).toBeVisible({ timeout: TIMEOUT })
}

test.describe.configure({ mode: 'serial' })

test('flow-04 线下服务履约：客户合同开单 → 采购确认 → 仅推荐采购 → 服务履约 → 销售验收', async ({ browser }) => {
  test.setTimeout(8 * 60 * 1000)

  const stamp = Date.now().toString(36).toUpperCase()
  const legalName = `北京福尚云E2E服务${stamp}有限公司`
  const creditCode = uniqueCreditCode()
  const contractNo = `HT-E2E-SVC-${stamp}`
  const dueIso = isoDate(30)
  const todayIso = isoDate(0)

  const admin = await openSession(browser, 'admin')
  const sales = await openSession(browser, 'sales')
  const procurement = await openSession(browser, 'procurement')
  const finance = await openSession(browser, 'finance')

  try {
    // 1. 主数据：销售开单需要已解析的采购负责人
    await ensureProcurementDispatcher(admin.page)

    // 2. 销售创建客户
    await gotoHeading(sales.page, '/sales/customers', '客户中心')
    await sales.page.getByRole('button', { name: '新建客户' }).click()
    await expect(sales.page.getByRole('heading', { name: '新建客户' })).toBeVisible({ timeout: TIMEOUT })
    await sales.page.getByLabel('法定名称').fill(legalName)
    await sales.page.getByLabel('统一社会信用代码').fill(creditCode)
    await sales.page.getByRole('button', { name: '创建客户' }).click()
    await expectToast(sales.page, '客户已创建')
    await expect(sales.page.getByRole('heading', { name: '新建客户' })).toBeHidden({ timeout: TIMEOUT })
    await sales.page.getByLabel('搜索客户').fill(legalName)
    await sales.page.getByLabel('搜索客户').press('Enter')
    await expect(sales.page.getByRole('link', { name: legalName })).toBeVisible({ timeout: TIMEOUT })

    // 3. 销售上传合同 PDF（系统不新建合同正文）
    await gotoHeading(sales.page, '/sales/contracts', '合同')
    await sales.page.getByRole('button', { name: '上传合同 PDF' }).click()
    await expect(sales.page.getByRole('heading', { name: '上传合同 PDF' })).toBeVisible({ timeout: TIMEOUT })
    await sales.page.locator('#card-contracts-upload-pdf-input').setInputFiles(await contractPdf())
    await sales.page.getByLabel('合同编号').fill(contractNo)
    await searchRemoteCombobox(sales.page, '客户', legalName, new RegExp(legalName))
    await expect(sales.page.locator('#card-contracts-upload-settlement-party')).not.toHaveValue('', {
      timeout: TIMEOUT,
    })
    await sales.page.getByRole('button', { name: '上传并归档' }).click()
    await expect(sales.page.getByRole('heading', { name: '上传合同 PDF' })).toBeHidden({ timeout: TIMEOUT })
    await expect(sales.page.getByRole('link', { name: contractNo })).toBeVisible({ timeout: TIMEOUT })

    // 4. 销售开线下服务销售单并提交（采购确认节点不选供给）
    await gotoHeading(sales.page, '/sales/orders', '销售单')
    await sales.page.locator('#sales-orders-list-header-create').click()
    await expect(sales.page.getByRole('heading', { name: '单据头' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByLabel('业务性质')).toBeVisible({ timeout: TIMEOUT })
    const contractPicker = sales.page.getByPlaceholder('搜索合同编号或客户')
    await contractPicker.click()
    await contractPicker.fill(contractNo)
    const contractOption = sales.page
      .getByRole('option', { name: new RegExp(contractNo) })
      .or(sales.page.locator('[data-slot="combobox-item"]').filter({ hasText: contractNo }))
      .first()
    await expect(contractOption).toBeVisible({ timeout: TIMEOUT })
    await contractOption.click()
    await expect(sales.page.getByText(legalName).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByLabel('负责销售')).not.toHaveValue('', { timeout: TIMEOUT })
    await chooseOption(sales.page, '福利场景', '年节礼包')

    await sales.page.getByRole('button', { name: '选择商品' }).first().click()
    await expect(sales.page.getByRole('heading', { name: '选择商品' })).toBeVisible({ timeout: TIMEOUT })
    const skuSearch = sales.page.getByPlaceholder('搜索 SKU、商品名称、编号或规格')
    await skuSearch.fill(SERVICE_SKU_NO)
    await skuSearch.press('Enter')
    await expect(sales.page.getByText(SERVICE_SKU_NO)).toBeVisible({ timeout: TIMEOUT })
    await sales.page.getByRole('checkbox', { name: new RegExp(`选择 ${SERVICE_SKU_NAME}`) }).click()
    await sales.page.locator('#sales-orders-sku-picker-confirm').click()
    await expect(sales.page.getByRole('heading', { name: '选择商品' })).toBeHidden({ timeout: TIMEOUT })
    await expect(sales.page.getByText(SERVICE_SKU_NAME).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.locator('[data-testid^="sales-line-procurement-owner-"]')).not.toContainText(
      '暂未确定采购负责人',
      { timeout: TIMEOUT },
    )

    await sales.page.getByLabel('数量').fill('2')
    await pickLabeledDate(sales.page, '批量交期', dueIso)
    await sales.page.locator('#sales-orders-create-batch-due-date-apply').click()
    await expectToast(sales.page, '已批量设置交期')

    await sales.page.locator('#sales-orders-create-submit').click()
    await expect(sales.page.getByRole('heading', { name: '提交销售单' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByText('审批中').first()).toBeVisible()
    await sales.page.locator('#sales-orders-submit-confirm-confirm').click()
    await sales.page.waitForURL(/\/sales\/orders\/[^/?]+/, { timeout: TIMEOUT })
    await expect(sales.page.getByText('审批中').first()).toBeVisible({ timeout: TIMEOUT })
    const salesOrderUrl = sales.page.url()
    const salesOrderId = salesOrderUrl.match(/\/sales\/orders\/([^/?]+)/)?.[1] ?? ''
    expect(salesOrderId).toBeTruthy()
    const orderNo = (await sales.page.locator('span.num.text-foreground').first().innerText()).trim()
    expect(orderNo.length).toBeGreaterThan(2)

    // 负向：提交人不得审批自己的销售单
    await refreshWorkspace(sales.page)
    await sales.page.locator('#workspace-queue-scope-started').click()
    await searchWorkspace(sales.page, orderNo)
    await expect(sales.page.getByRole('button', { name: '通过', exact: true })).toHaveCount(0)

    // 负向：采购确认通过前不得履约、不得供给分配、不得关闭
    await refreshWorkspace(procurement.page)
    await procurement.page.locator('#workspace-family-nav-fulfillment').click()
    await searchWorkspace(procurement.page, orderNo)
    await expect(procurement.page.getByRole('button', { name: /履约处理|客户验收登记/ })).toHaveCount(0)
    await procurement.page.locator('#workspace-family-nav-procurement').click()
    await searchWorkspace(procurement.page, orderNo)
    await expect(procurement.page.getByRole('button', { name: /待供给分配/ })).toHaveCount(0)

    // 5. 采购在 W01 原地通过销售单审批（采购确认节点不选供给、不录入成本）
    await openInboxTask(procurement.page, 'approval', /销售单审批/, orderNo)
    await expect(procurement.page.getByText('采购确认').first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByRole('button', { name: '通过', exact: true })).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByTestId('purchase-create-preview')).toHaveCount(0)
    await approveCurrentTask(procurement.page, '采购确认')

    await sales.page.goto(`/sales/orders/${salesOrderId}`)
    const identity = sales.page.getByRole('heading', { name: legalName }).locator('xpath=..')
    await expect(identity.getByText('已生效')).toBeVisible({ timeout: TIMEOUT })
    await expect(identity.getByText('已关闭')).toHaveCount(0)
    await expect(sales.page.locator('#sales-orders-detail-start-change')).toBeVisible()

    // 负向：销售单刚生效时不得履约（须先完成供给分配且采购单生效）
    await refreshWorkspace(procurement.page)
    await procurement.page.locator('#workspace-family-nav-fulfillment').click()
    await searchWorkspace(procurement.page, orderNo)
    await expect(procurement.page.getByRole('button', { name: /履约处理/ })).toHaveCount(0)

    // 6. 供给分配：线下服务只能推荐采购，不得分配现有库存；确认后立即提交采购单
    if ((await procurement.page.getByRole('heading', { name: '供给分配' }).count()) === 0) {
      await openInboxTask(procurement.page, 'procurement', /待供给分配/, orderNo)
    }
    await expect(procurement.page.getByRole('heading', { name: '供给分配' })).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText(orderNo).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText(SERVICE_SKU_NAME).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('将建立库存预留').locator('xpath=..')).toContainText('0 条')
    await expect(procurement.page.getByText('将创建采购单').locator('xpath=..')).toContainText('1 张')
    await expect(procurement.page.getByText('现货')).toHaveCount(0)
    await expect(procurement.page.getByText(/线下服务/).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('不适用').first()).toBeVisible()

    const deliveryPicker = procurement.page.getByLabel('预计交付日')
    if ((await deliveryPicker.count()) > 0) {
      const current = await deliveryPicker.getAttribute('aria-label')
      if (!current || current.includes('选择日期')) {
        await deliveryPicker.click()
        await pickCalendarIso(procurement.page, todayIso)
      }
    }

    await procurement.page.locator('#procurement-orders-create-preview').click()
    await expect(procurement.page.getByRole('heading', { name: '预览供给分配' })).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('本次不占用现有库存')).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('现有库存分配')).toHaveCount(0)
    await expect(procurement.page.getByText('本次全部由现有库存满足，不会创建采购单。')).toHaveCount(0)
    await procurement.page.locator('#procurement-orders-create-preview-confirm').click()
    await expect(procurement.page.getByRole('heading', { name: '确认供给分配' })).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText(/创建 1 张采购单提交审批/)).toBeVisible()
    await procurement.page.locator('#procurement-orders-create-confirm').click()
    await expectToast(procurement.page, '供给分配已完成')
    await expect(procurement.page.getByText('已创建 1 张采购单并提交审批。')).toBeVisible({ timeout: TIMEOUT })

    // 负向：不得留下未提交草稿；不得零张采购单
    await gotoHeading(procurement.page, '/procurement/orders', '采购单')
    await procurement.page.locator('#procurement-orders-list-search').fill(orderNo)
    await procurement.page.locator('#procurement-orders-list-search').press('Enter')
    await expect(procurement.page.getByText('审批中').first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('线下服务').first()).toBeVisible()
    await expect(procurement.page.getByText('草稿', { exact: true })).toHaveCount(0)

    // 负向：采购单生效前仍不得服务履约
    await refreshWorkspace(procurement.page)
    await procurement.page.locator('#workspace-family-nav-fulfillment').click()
    await searchWorkspace(procurement.page, orderNo)
    await expect(procurement.page.getByRole('button', { name: /履约处理/ })).toHaveCount(0)

    // 7. 财务总监审批采购单（caigou 提交，caiwu 审批）
    await openInboxTask(finance.page, 'approval', /采购单审批/, orderNo)
    await expect(finance.page.getByText('财务总监审批').first()).toBeVisible({ timeout: TIMEOUT })
    await approveCurrentTask(finance.page, '财务总监审批')

    await gotoHeading(procurement.page, '/procurement/orders', '采购单')
    await procurement.page.locator('#procurement-orders-list-search').fill(orderNo)
    await procurement.page.locator('#procurement-orders-list-search').press('Enter')
    await expect(procurement.page.getByText('已生效').first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('草稿', { exact: true })).toHaveCount(0)

    // 8. 采购登记服务履约（对象由销售明细锁定；时间/地点/结果/凭证）
    await refreshWorkspace(procurement.page)
    await procurement.page.locator('#workspace-family-nav-fulfillment').click()
    await searchWorkspace(procurement.page, orderNo)
    await expect(procurement.page.getByRole('button', { name: /履约处理/ }).first()).toBeVisible({ timeout: TIMEOUT })
    await procurement.page.getByRole('button', { name: /履约处理/ }).first().click()
    await expect(procurement.page.getByRole('heading', { name: new RegExp(`线下服务.*${orderNo}|线下服务`) })).toBeVisible({
      timeout: TIMEOUT,
    })
    await expect(procurement.page.getByLabel('线下服务表单')).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByRole('button', { name: '通过', exact: true })).toHaveCount(0)
    await expect(procurement.page.getByRole('button', { name: '驳回', exact: true })).toHaveCount(0)
    await expect(procurement.page.getByText('审批摘要')).toHaveCount(0)

    await procurement.page.getByLabel('本次完成数量').fill('2')
    await chooseOption(procurement.page, '履约结果', '成功')
    await procurement.page.getByLabel('服务时间').click()
    await pickCalendarIso(procurement.page, todayIso)
    await pickCalendarIso(procurement.page, todayIso)
    await procurement.page.getByLabel('开始时间').fill('09:00')
    await procurement.page.getByLabel('结束时间').fill('11:00')
    const timeDone = procurement.page.locator('#fulfillment-operations-service-form-service-time-done')
    if (await timeDone.isDisabled()) {
      await pickCalendarIso(procurement.page, todayIso)
    }
    await timeDone.click()
    await procurement.page.getByLabel('服务地点').fill('北京市大兴区旧宫镇客户现场')
    await procurement.page.locator('#fulfillment-operations-service-form-evidence-input').setInputFiles({
      name: 'service-evidence.png',
      mimeType: 'image/png',
      buffer: PNG_1X1,
    })
    await procurement.page.getByLabel('完成说明').fill('已上门安装并完成现场验收')
    await procurement.page.locator('#fulfillment-operations-work-surface-confirm').click()
    await expect(procurement.page.getByRole('heading', { name: '确认服务完成？' })).toBeVisible({ timeout: TIMEOUT })
    await procurement.page.locator('#fulfillment-operations-workspace-confirm-confirm').click()
    await expect(procurement.page.getByRole('heading', { name: '确认服务完成？' })).toBeHidden({
      timeout: TIMEOUT,
    })
    await expect(procurement.page.getByRole('button', { name: '通过', exact: true })).toHaveCount(0)
    await expect(procurement.page.getByText('当前节点')).toHaveCount(0)

    // 9. 销售在 W01 登记客户验收（NO_APPROVAL）
    await openInboxTask(sales.page, 'fulfillment', /客户验收登记/, orderNo)
    await expect(sales.page.getByRole('button', { name: '登记客户验收' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByRole('button', { name: '通过', exact: true })).toHaveCount(0)
    await sales.page.locator('#sales-orders-acceptance-register-open').click()
    await expect(sales.page.getByRole('heading', { name: '登记客户验收' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByText('服务履约').first()).toBeVisible({ timeout: TIMEOUT })
    await sales.page.locator('#sales-orders-acceptance-register-submit').click()
    await expect(sales.page.getByRole('heading', { name: '确认客户验收' })).toBeVisible({ timeout: TIMEOUT })
    await sales.page.locator('#sales-orders-acceptance-confirm-confirm').click()
    await expectToast(sales.page, '客户验收已登记')
    await expect(sales.page.getByRole('heading', { name: '登记客户验收' })).toBeHidden({ timeout: TIMEOUT })
    await expect(sales.page.getByRole('button', { name: '通过', exact: true })).toHaveCount(0)

    // 10. 里程碑：履约完成但应收未结清，销售单不得关闭；本流程未开变更单
    await sales.page.goto(`/sales/orders/${salesOrderId}`)
    const closingIdentity = sales.page.getByRole('heading', { name: legalName }).locator('xpath=..')
    await expect(closingIdentity.getByText('已生效')).toBeVisible({ timeout: TIMEOUT })
    await expect(closingIdentity.getByText('已关闭')).toHaveCount(0)
    await expect(sales.page.getByText('履约').locator('xpath=..').getByText('已完成')).toBeVisible({
      timeout: TIMEOUT,
    })
    await expect(sales.page.getByText('改单草稿')).toHaveCount(0)
    await expect(sales.page.getByText('销售变更单审批')).toHaveCount(0)
    await expect(sales.page.locator('#sales-orders-detail-start-change')).toBeVisible()
  } finally {
    await Promise.all([
      admin.context.close(),
      sales.context.close(),
      procurement.context.close(),
      finance.context.close(),
    ])
  }
})
