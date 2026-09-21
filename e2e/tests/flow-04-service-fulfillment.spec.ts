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
 * - 客户验收成功 toast 描述使用「已过账」（与 docs/erp-phase-1.md §10.1 不用「过账」不一致）
 * - ServiceFulfillment / CustomerAcceptance 为 NO_APPROVAL，工作台原地确认，不出现审批决定栏
 */
import { existsSync } from 'node:fs'
import path from 'node:path'
import { expect, test, type Page } from '@playwright/test'

import { createCustomerViaUi } from '../helpers/customers'
import { openLoggedInWorkspace } from '../helpers/login'
import { ensureDefaultProcurementOwner } from '../helpers/procurement'
import { expandSourcingEditor } from '../helpers/sourcing'
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
} from '../helpers/ui'

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

function isoDate(offsetDays = 0): string {
  const date = new Date()
  date.setDate(date.getDate() + offsetDays)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function uniqueCreditCode(): string {
  // 统一社会信用代码必须恰好 18 位字母或数字，否则创建按钮保持禁用。
  const stamp = Date.now().toString().slice(-8)
  return `91110108MA${stamp}`
}

async function stableGoto(page: Page, href: string) {
  // 开发热更新或上一个导航未落定会中断 goto（ERR_ABORTED）：短等待后重试，最多 3 次。
  for (let i = 0; ; i += 1) {
    try {
      await page.goto(href)
      return
    } catch (error) {
      if (i >= 2) throw error
      await page.waitForTimeout(2_000)
    }
  }
}

async function gotoHeading(page: Page, href: string, heading: string | RegExp) {
  await stableGoto(page, href)
  await expect(page.getByRole('heading', { name: heading })).toBeVisible({ timeout: TIMEOUT })
}

async function clickWithoutToastOverlay(
  page: Page,
  target: import('@playwright/test').Locator,
  settled?: () => Promise<boolean>,
) {
  // Toast 可能在关闭后再次出现导致遮挡：循环关闭后短超时点按，成功即返回（与 flow-03 同款）。
  // 对话框可能在某次点按后关闭（提交成功）：每轮先检查 settled，关了就直接返回。
  for (let i = 0; i < 8; i += 1) {
    if (settled && (await settled().catch(() => false))) return
    // 鼠标停在 toast 上会暂停其自动消失：先移开再关闭，避免遮挡常驻。
    await page.mouse.move(8, 8).catch(() => undefined)
    await dismissToasts(page)
    try {
      await target.click({ timeout: 3_000 })
      return
    } catch {
      // 被遮挡则下一轮重试；8 轮都不成功改走 DOM 派发。
    }
  }
  if (settled && (await settled().catch(() => false))) return
  // 悬浮提示持续遮挡导致真实点击无法命中：改走 DOM 直接派发点击，绕过覆盖层。
  await target.dispatchEvent('click')
}

async function contractPdf(): Promise<string | { name: string; mimeType: string; buffer: Buffer }> {
  const fixture = path.join(process.cwd(), 'fixtures', 'sample-contract.pdf')
  if (existsSync(fixture)) return fixture
  return { name: 'sample-contract.pdf', mimeType: 'application/pdf', buffer: MINIMAL_PDF }
}

async function refreshWorkspace(page: Page) {
  await stableGoto(page, '/workspace')
  await expect(page.getByRole('heading', { name: '我的工作台' })).toBeVisible({ timeout: TIMEOUT })
  const refresh = page.locator('#workspace-home-refresh')
  if (await refresh.isVisible()) await refresh.click()
}

async function clearWorkspaceSearch(page: Page) {
  const search = page.locator('#workspace-queue-toolbar-search-input')
  // 工作台筛选栏当前暂停展示；无搜索框时只刷新队列。
  if (await search.isVisible().catch(() => false)) {
    await search.fill('')
    await search.press('Enter')
  }
  const refresh = page.locator('#workspace-home-refresh').or(page.getByRole('button', { name: '刷新', exact: true }))
  if (await refresh.first().isVisible().catch(() => false)) {
    await refresh.first().click()
  }
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

  const admin = await openLoggedInWorkspace(browser, 'admin')
  const sales = await openLoggedInWorkspace(browser, 'xiaoshou')
  const procurement = await openLoggedInWorkspace(browser, 'caigou')
  const finance = await openLoggedInWorkspace(browser, 'caiwu')

  try {
    // 1. 主数据：销售开单需要已解析的采购负责人
    await ensureDefaultProcurementOwner(admin.page)

    // 2. 销售创建客户
    await createCustomerViaUi(sales.page, {
      legalName,
      shortName: `E2E服务${stamp.slice(-6)}`,
      creditCode,
      paymentTermLabel: '货到 15 天',
      contact: { name: '李测', phone: '13800138001' },
      address: '北京市朝阳区测试路 1 号',
    })
    await expect(sales.page.getByRole('link', { name: `E2E服务${stamp.slice(-6)}` })).toBeVisible({
      timeout: TIMEOUT,
    })

    // 3. 销售上传合同 PDF（系统不新建合同正文）
    await gotoHeading(sales.page, '/sales/contracts', /^合同$/)
    await sales.page.locator('#page-actions-action-upload').click()
    await expect(sales.page.getByRole('heading', { name: '上传合同 PDF' })).toBeVisible({ timeout: TIMEOUT })
    await sales.page.locator('#card-contracts-upload-pdf-input').setInputFiles(await contractPdf())
    // 同页表头排序/拖拽柄的无障碍名也含“合同编号”：用上传框内稳定 id。
    await sales.page.locator('#card-contracts-upload-contract-no').fill(contractNo)
    // 同页其他“客户”标签会干扰 label 定位：用上传框内客户下拉稳定 id。
    await chooseOption(
      sales.page,
      sales.page.locator('#card-contracts-upload-customer'),
      new RegExp(legalName),
      legalName,
    )
    await expect(sales.page.locator('#card-contracts-upload-settlement-party')).not.toHaveValue('', {
      timeout: TIMEOUT,
    })
    await sales.page.getByRole('button', { name: '上传并归档' }).click()
    await expect(sales.page.getByRole('heading', { name: '上传合同 PDF' })).toBeHidden({ timeout: TIMEOUT })
    // 合同列表行是按钮（打开合同），不是链接。
    await expect(sales.page.getByRole('button', { name: `打开合同 ${contractNo}` })).toBeVisible({ timeout: TIMEOUT })

    // 4. 销售开线下服务销售单并提交（采购确认节点不选供给）
    // 列表页同时有 h1「销售单」与 h2「销售单 N 条」：锚定全名避开严格模式。
    await gotoHeading(sales.page, '/sales/orders', /^销售单$/)
    await sales.page.locator('#sales-orders-list-header-create').click()
    await expect(
      sales.page
        .getByRole('heading', { name: '新建销售单' })
        .or(sales.page.getByRole('heading', { name: '业务信息' })),
    ).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.locator('#sales-orders-create-contract')).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByLabel('业务性质')).toBeVisible({ timeout: TIMEOUT })
    await chooseOption(
      sales.page,
      sales.page.locator('#sales-orders-create-contract'),
      new RegExp(contractNo),
      contractNo,
    )
    await expect(sales.page.getByText(legalName).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByLabel('负责销售')).not.toHaveValue('', { timeout: TIMEOUT })
    await chooseOption(sales.page, sales.page.locator('#sales-orders-create-header-welfare-scene'), '年节礼包')
    // 付款条件必填缺一不可，否则提交按钮保持禁用（与 flow-03 同款）。
    await chooseOption(
      sales.page,
      sales.page.locator('#sales-orders-create-header-payment-terms'),
      '货到 30 天',
    )

    await sales.page.getByRole('button', { name: '添加商品' }).first().click()
    await expect(sales.page.getByRole('heading', { name: '添加商品' })).toBeVisible({ timeout: TIMEOUT })
    const skuSearch = sales.page.getByPlaceholder('搜索 SKU、商品名称、编号或规格')
    await skuSearch.fill(SERVICE_SKU_NO)
    await skuSearch.press('Enter')
    // 搜索框会把关键词回显为筛选 chips（搜索：…）：用精确匹配只认表格行。
    await expect(sales.page.getByText(SERVICE_SKU_NO, { exact: true })).toBeVisible({ timeout: TIMEOUT })
    await sales.page.getByRole('checkbox', { name: new RegExp(`选择 ${SERVICE_SKU_NAME}`) }).click()
    await sales.page.locator('#sales-orders-sku-picker-confirm').click()
    await expect(sales.page.getByRole('heading', { name: '添加商品' })).toBeHidden({ timeout: TIMEOUT })
    await expect(sales.page.getByText(SERVICE_SKU_NAME).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.locator('[data-testid^="sales-line-procurement-owner-"]')).not.toContainText(
      '暂未确定采购负责人',
      { timeout: TIMEOUT },
    )

    await sales.page.getByLabel('数量').fill('2')
    // 批量交期控件改名：用稳定 id 打开日历再跨月点选（与 flow-03 同款）。
    await sales.page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(sales.page, sales.page.locator('#sales-orders-create-batch-due-date'), dueIso)
    await sales.page.locator('#sales-orders-create-batch-due-date-apply').click()
    await expectToast(sales.page, '已批量设置交期')

    await clickWithoutToastOverlay(
      sales.page,
      sales.page.locator('#sales-orders-create-submit'),
      async () =>
        await sales.page
          .getByRole('heading', { name: '提交销售单' })
          .isVisible()
          .catch(() => false),
    )
    await expect(sales.page.getByRole('heading', { name: '提交销售单' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByText('审批中').first()).toBeVisible()
    // 批量交期 toast 常盖住确认按钮：先清 toast 再点，盖住不散时走 DOM 派发。
    // 确认点按后对话框关闭即提交成功：透过 settled 提前返回，避免对已卸载按钮重试。
    const submitDialog = sales.page.getByRole('dialog', { name: '提交销售单' })
    await clickWithoutToastOverlay(sales.page, sales.page.locator('#sales-orders-submit-confirm-confirm'), async () => {
      await sales.page.waitForURL(/\/sales\/orders\/[^/?]+/, { timeout: 2_000 }).catch(() => undefined)
      return !/\/sales\/orders\?mode=create/.test(sales.page.url())
    })
    await expect(submitDialog).toBeHidden({ timeout: TIMEOUT })
    await sales.page.waitForURL(/\/sales\/orders\/[^/?]+/, { timeout: TIMEOUT })
    await expect(sales.page.getByText('审批中').first()).toBeVisible({ timeout: TIMEOUT })
    const salesOrderUrl = sales.page.url()
    const salesOrderId = salesOrderUrl.match(/\/sales\/orders\/([^/?]+)/)?.[1] ?? ''
    expect(salesOrderId).toBeTruthy()
    const orderNo = await readHeaderDocumentNumber(sales.page)
    expect(orderNo.length).toBeGreaterThan(2)

    // 负向：提交人不得审批自己的销售单
    await refreshWorkspace(sales.page)
    await sales.page.locator('#workspace-queue-scope-started').click()
    await clearWorkspaceSearch(sales.page)
    await expect(sales.page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)

    // 负向：采购确认通过前不得履约、不得供给分配、不得关闭
    await refreshWorkspace(procurement.page)
    await selectWorkspaceFamily(procurement.page, "fulfillment")
    await clearWorkspaceSearch(procurement.page)
    await expect(procurement.page.getByRole('button', { name: /履约处理|客户验收登记/ })).toHaveCount(0)
    await selectWorkspaceFamily(procurement.page, "procurement")
    await clearWorkspaceSearch(procurement.page)
    await expect(procurement.page.getByRole('button', { name: /待供给分配/ })).toHaveCount(0)

    // 5. 采购在 W01 原地通过销售单审批（采购确认节点不选供给、不录入成本）
    await openWorkspaceTask(procurement.page, /销售单审批/, orderNo, 'approval')
    await expect(procurement.page.getByText('采购确认').first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByRole('button', { name: /^(通过|同意审批)$/ })).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByTestId('purchase-create-preview')).toHaveCount(0)
    await approveCurrentDocument(procurement.page)

    await sales.page.goto(`/sales/orders/${salesOrderId}`)
    // 审批通过后生效异步落定：刷新轮询直到出现已生效，最长约 2 分钟。
    const identity = sales.page.getByRole('heading', { name: legalName }).locator('xpath=..')
    let effective = false
    for (let i = 0; i < 12; i += 1) {
      await sales.page.reload()
      await expect(sales.page.getByRole('heading', { name: legalName })).toBeVisible({ timeout: TIMEOUT })
      if (await identity.getByText('已生效').count()) {
        effective = true
        break
      }
      await sales.page.waitForTimeout(10_000)
    }
    expect(effective).toBe(true)
    await expect(identity.getByText('已生效')).toBeVisible({ timeout: TIMEOUT })
    await expect(identity.getByText('已关闭')).toHaveCount(0)
    await expect(sales.page.locator('#sales-orders-detail-start-change')).toBeVisible()

    // 负向：销售单刚生效时不得履约（须先完成供给分配且采购单生效）
    await refreshWorkspace(procurement.page)
    await selectWorkspaceFamily(procurement.page, "fulfillment")
    await clearWorkspaceSearch(procurement.page)
    await expect(procurement.page.getByRole('button', { name: /履约处理/ })).toHaveCount(0)

    // 6. 供给分配：线下服务只能推荐采购，不得分配现有库存；确认后立即提交采购单
    if ((await procurement.page.getByRole('heading', { name: '供给分配' }).count()) === 0) {
      await openWorkspaceTask(procurement.page, /待供给分配/, orderNo, 'procurement')
    }
    await expect(procurement.page.getByRole('heading', { name: '供给分配' })).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText(orderNo).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText(SERVICE_SKU_NAME).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('将建立库存预留').locator('xpath=..')).toContainText('0 条')
    await expect(procurement.page.getByText('将创建采购单').locator('xpath=..')).toContainText('1 张')
    await expect(procurement.page.getByText('现货')).toHaveCount(0)
    await expandSourcingEditor(procurement.page, SERVICE_SKU_NAME)
    await expect(
      procurement.page.getByText('线下服务').filter({ visible: true }).first(),
    ).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.locator('[id$="-warehouse"]')).toHaveCount(0)

    const deliveryPicker = procurement.page.getByLabel('预计交付日')
    if ((await deliveryPicker.count()) > 0) {
      const current = await deliveryPicker.getAttribute('aria-label')
      if (!current || current.includes('选择日期')) {
        await pickCalendarDay(procurement.page, deliveryPicker, todayIso)
      }
    }

    await procurement.page.locator('#procurement-orders-create-preview').click()
    await expect(procurement.page.getByRole('heading', { name: '预览供给分配' })).toBeVisible({ timeout: TIMEOUT })
    await expect(
      procurement.page.getByText(/本次不占用现有库存|将为供给缺口创建|张采购单提交审批/),
    ).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('现有库存分配')).toHaveCount(0)
    await expect(procurement.page.getByText('本次全部由现有库存满足，不会创建采购单。')).toHaveCount(0)
    // 预览确认按钮同样可能因重渲染不稳定：跳过稳定帧直接强制点击。
    const waiting = procurement.page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        response.url().includes('/admin/purchase-orders/from-sourcing'),
      { timeout: TIMEOUT },
    )
    await procurement.page.locator('#procurement-orders-create-preview-confirm').click({ force: true })
    const submitResponse = await waiting
    expect(submitResponse.ok()).toBe(true)
    await expect(procurement.page.getByText('已创建 1 张采购单并提交审批。')).toBeVisible({ timeout: TIMEOUT })

    // 负向：不得留下未提交草稿；不得零张采购单
    await gotoHeading(procurement.page, '/procurement/orders', '采购单')
    await procurement.page.locator('#procurement-orders-list-search').fill(orderNo)
    await procurement.page.locator('#procurement-orders-list-search').press('Enter')
    await expect(procurement.page.getByRole('table').getByText('审批中', { exact: true }).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByText('线下服务').first()).toBeVisible()
    await expect(procurement.page.getByRole('table').getByText('草稿', { exact: true })).toHaveCount(0)

    // 负向：采购单生效前仍不得服务履约
    await refreshWorkspace(procurement.page)
    await selectWorkspaceFamily(procurement.page, "fulfillment")
    await clearWorkspaceSearch(procurement.page)
    await expect(procurement.page.getByRole('button', { name: /履约处理/ })).toHaveCount(0)

    // 7. 财务总监审批采购单（caigou 提交，caiwu 审批）
    await openWorkspaceTask(finance.page, /采购单审批/, orderNo, 'approval')
    await expect(finance.page.getByText('财务总监审批').first()).toBeVisible({ timeout: TIMEOUT })
    await approveCurrentDocument(finance.page)

    await gotoHeading(procurement.page, '/procurement/orders', '采购单')
    await procurement.page.locator('#procurement-orders-list-search').fill(orderNo)
    await procurement.page.locator('#procurement-orders-list-search').press('Enter')
    await expect(procurement.page.getByRole('table').getByText('已生效', { exact: true }).first()).toBeVisible({ timeout: TIMEOUT })
    await expect(procurement.page.getByRole('table').getByText('草稿', { exact: true })).toHaveCount(0)

    // 8. 采购登记服务履约（对象由销售明细锁定；时间/地点/结果/凭证）
    await refreshWorkspace(procurement.page)
    // 工作台轮询可能在填写期间把任务面板刷掉（family 切回全部、表单卸载）：整个填写包三轮重试，
    // 每轮先确保面板打开再重填（覆盖写等幂，草稿存在也不怕）。
    for (let attempt = 0; ; attempt += 1) {
      if ((await procurement.page.locator('[aria-label="线下服务表单"]').count()) === 0) {
        await openWorkspaceTask(procurement.page, /履约处理/, orderNo, 'fulfillment')
        await openFulfillmentWorkspaceForm(procurement.page)
        // 任务标题使用采购单身份，实际履约表单必须为线下服务。
        await expect(procurement.page.locator('[aria-label="线下服务表单"]')).toBeVisible({
          timeout: TIMEOUT,
        })
      }
      try {
        await expect(procurement.page.locator('[aria-label="线下服务表单"]')).toBeVisible({ timeout: 10_000 })
        await procurement.page.getByLabel('本次完成数量').fill('2')
        await chooseOption(
          procurement.page,
          procurement.page.locator('#fulfillment-operations-service-form-result'),
          '成功',
        )
        // 服务时间是 DateTimeRangeLocalPicker：触发按钮的 aria-label 是占位文案而非「服务时间」，
        // getByLabel 会命中 label 元素本身导致点击超时；改走稳定 id。悬浮 toast 会吞点击，
        // 用强制点击穿透（弹层本来就盖在最上）。
        // 日历点选当天一次即落定起止同一天（截图证实触发器已显示完整范围）：直接用全局 id 定位，
        // 不经过 popover 链（同页可能残留其他弹层导致 last() 错位）。第二次点选反而会重置范围。
        // 弹层开关受工作台轮询干扰偶发打不开：轮询触发器直到当天按钮出现，最多 4 轮。
        const serviceDay = procurement.page.locator(`button[id$="-day-${todayIso}"]:not([disabled])`).first()
        for (let round = 0; round < 4; round += 1) {
          await procurement.page.locator('#fulfillment-operations-service-form-service-time').click({ force: true })
          if (await serviceDay.isVisible({ timeout: 5_000 }).catch(() => false)) break
        }
        await expect(serviceDay).toBeVisible({ timeout: 10_000 })
        await serviceDay.click({ force: true })
        const fromTime = procurement.page.locator('#fulfillment-operations-service-form-service-time-from-time')
        const toTime = procurement.page.locator('#fulfillment-operations-service-form-service-time-to-time')
        await expect(fromTime).toBeEnabled({ timeout: 10_000 })
        await expect(toTime).toBeEnabled({ timeout: 10_000 })
        await fromTime.fill('09:00')
        await toTime.fill('11:00')
        const timeDone = procurement.page.locator('#fulfillment-operations-service-form-service-time-done')
        await expect(timeDone).toBeEnabled({ timeout: 10_000 })
        await timeDone.click({ force: true })
        await procurement.page.getByLabel('服务地点').fill('北京市大兴区旧宫镇客户现场')
        await procurement.page.locator('#fulfillment-operations-service-form-evidence-input').setInputFiles({
          name: 'service-evidence.png',
          mimeType: 'image/png',
          buffer: PNG_1X1,
        })
        await procurement.page.getByLabel('完成说明').fill('已上门安装并完成现场验收')
        break
      } catch (error) {
        if (attempt >= 2) throw error
      }
    }
    await expect(procurement.page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)
    await expect(procurement.page.getByRole('button', { name: '驳回', exact: true })).toHaveCount(0)
    await expect(procurement.page.getByText('审批摘要')).toHaveCount(0)
    await procurement.page.locator('#fulfillment-operations-work-surface-confirm').click()
    await expect(procurement.page.getByRole('heading', { name: '确认服务完成？' })).toBeVisible({ timeout: TIMEOUT })
    await procurement.page.locator('#fulfillment-operations-workspace-confirm-confirm').click()
    await expect(procurement.page.getByRole('heading', { name: '确认服务完成？' })).toBeHidden({
      timeout: TIMEOUT,
    })
    await expect(procurement.page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)
    await expect(procurement.page.getByText('当前节点')).toHaveCount(0)

    // 9. 销售在 W01 登记客户验收（NO_APPROVAL）
    await openWorkspaceTask(sales.page, /客户验收登记/, orderNo, 'fulfillment')
    await expect(sales.page.getByRole('button', { name: '登记客户验收' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)
    await sales.page.locator('#sales-orders-acceptance-register-open').click()
    await expect(sales.page.getByRole('heading', { name: '登记客户验收' })).toBeVisible({ timeout: TIMEOUT })
    await expect(sales.page.getByText('服务履约').first()).toBeVisible({ timeout: TIMEOUT })
    await sales.page.locator('#sales-orders-acceptance-register-submit').click()
    await expect(sales.page.getByRole('heading', { name: '确认客户验收' })).toBeVisible({ timeout: TIMEOUT })
    await sales.page.locator('#sales-orders-acceptance-confirm-confirm').click()
    await expectToast(sales.page, '客户验收已登记')
    await expect(sales.page.getByRole('heading', { name: '登记客户验收' })).toBeHidden({ timeout: TIMEOUT })
    await expect(sales.page.getByRole('button', { name: /^(通过|同意审批)$/ })).toHaveCount(0)

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
