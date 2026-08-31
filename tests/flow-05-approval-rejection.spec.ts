/**
 * 流程: [flow-05] 销售单审批驳回与三条出路
 * 文档: docs/erp-phase-1.md §4.4、§7.3.1；approval-workflow-contract.md §4.4.2–§4.4.4、§11
 * 账号: xiaoshou（提交）→ caigou（采购确认节点驳回/通过）；admin 仅补采购责任默认调度人
 *
 * 文档-代码差异（以代码为准）:
 * 1. 驳回后业务状态仍是审批中（IN_APPROVAL），作废接口只允许 DRAFT→VOIDED；
 *    前端销售单没有「作废」按钮，场景 C 必须先撤回再走 POST /admin/sales-orders/{id}/void。
 * 2. 驳回后禁止变更单：按钮「发起改单」仍渲染但 disabled，title=服务端 blocker。
 * 3. 页头「版本」展示的是 currentRevisionNo（尚未生效 / vN），不是审批 subject_version；
 *    subject_version 取详情 GET /admin/sales-orders/{id} 的 submissions[].submission_no。
 */
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { test, expect, type Browser, type BrowserContext, type Locator, type Page } from '@playwright/test'

import { ACCOUNTS } from '../helpers/accounts'
import { loginViaUi, newLoggedInContext } from '../helpers/login'

const VISIBLE = { timeout: 20_000 } as const
const API_BASE = process.env.API_BASE || 'http://127.0.0.1:10001'
const SKU_NAME = '狮峰明前龙井礼盒'
const REJECT_REASON = '无法履约，成本上涨，交期不满足'
const WITHDRAW_REASON = '与客户改数量后重提'
const VOID_HTTP_NOTE =
  '销售单草稿作废无页面按钮，走已发布 POST /admin/sales-orders/{id}/void'

type AccountCred = { account: string; password: string; name?: string }

type Session = { context: BrowserContext; page: Page }

type OrderSnapshot = {
  id: string
  orderNo: string
  quantity: string
  unitPrice: string
  submissionNo: number
}

function pickAccount(...keys: string[]): AccountCred {
  const bag = ACCOUNTS as Record<
    string,
    { account?: string; username?: string; password?: string; name?: string }
  >
  for (const key of keys) {
    const row = bag[key]
    if (row) {
      return {
        account: row.account ?? row.username ?? key,
        password: row.password ?? '123456',
        name: row.name,
      }
    }
    const found = Object.values(bag).find(
      (item) => item?.account === key || item?.username === key,
    )
    if (found) {
      return {
        account: found.account ?? found.username ?? key,
        password: found.password ?? '123456',
        name: found.name,
      }
    }
  }
  const fallback = keys[keys.length - 1] ?? 'xiaoshou'
  return { account: fallback, password: '123456' }
}

const SALES = pickAccount('sales', 'xiaoshou')
const PROCUREMENT = pickAccount('procurement', 'caigou')
const ADMIN = pickAccount('admin', 'admin')

function pad2(value: number): string {
  return String(value).padStart(2, '0')
}

function isoDate(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`
}

function addDays(base: Date, days: number): Date {
  const next = new Date(base)
  next.setDate(base.getDate() + days)
  return next
}

function uniqueCreditCode(): string {
  const stamp = Date.now().toString(36).toUpperCase().replace(/[^0-9A-Z]/g, '0')
  return `91110108MA01${stamp}`.slice(0, 18).padEnd(18, '0')
}

function contractPdfPath(): string {
  const repoPath = path.join(process.cwd(), 'fixtures', 'sample-contract.pdf')
  if (fs.existsSync(repoPath)) return repoPath
  const fallback = path.join(os.tmpdir(), 'erp-flow-05-sample-contract.pdf')
  if (!fs.existsSync(fallback)) {
    fs.writeFileSync(
      fallback,
      Buffer.from(
        '%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000052 00000 n \n0000000101 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n178\n%%EOF\n',
      ),
    )
  }
  return fallback
}

async function openSession(browser: Browser, creds: AccountCred): Promise<Session> {
  const opened = await newLoggedInContext(browser, creds)
  if (opened && typeof opened === 'object' && 'page' in opened) {
    return opened as Session
  }
  const context = await browser.newContext()
  const page = await context.newPage()
  await loginViaUi(page, creds)
  return { context, page }
}

async function selectCombobox(
  page: Page,
  input: Locator,
  optionLabel: string | RegExp,
): Promise<void> {
  const typed =
    typeof optionLabel === 'string'
      ? optionLabel
      : optionLabel.source.replace(/^\^|\$$/g, '').replace(/\\s\+/g, ' ')
  await input.click()
  await input.fill(typed)
  const option = page.getByRole('option', { name: optionLabel }).first()
  await expect(option).toBeVisible(VISIBLE)
  await option.click()
}

async function pickIsoDate(page: Page, fieldId: string, iso: string): Promise<void> {
  await page.locator(`#${fieldId}`).click()
  const day = page.locator(`[id$="-day-${iso}"]`).first()
  if ((await day.count()) === 0) {
    await page.locator(`#${fieldId}-calendar-next-month`).click()
  }
  await expect(page.locator(`[id$="-day-${iso}"]`).first()).toBeVisible(VISIBLE)
  await page.locator(`[id$="-day-${iso}"]`).first().click()
}

async function gotoNav(page: Page, name: string, href: string): Promise<void> {
  const link = page.getByRole('link', { name })
  if (await link.count()) {
    await link.first().click()
  } else {
    await page.goto(href)
  }
  await expect(page).toHaveURL(new RegExp(href.replace(/[?]/g, '\\?')), VISIBLE)
}

function submissionNoOf(detail: Record<string, unknown>): number {
  const submissions = (detail.submissions as Array<{ submission_no?: number }> | undefined) ?? []
  const latest = [...submissions].sort(
    (left, right) => (right.submission_no ?? 0) - (left.submission_no ?? 0),
  )[0]
  return Number(latest?.submission_no ?? 0)
}

async function bearerToken(page: Page): Promise<string> {
  const token = await page.evaluate(() => localStorage.getItem('erp.token'))
  expect(token, '登录 token 应写入 localStorage erp.token').toBeTruthy()
  return token as string
}

async function fetchSalesOrder(
  page: Page,
  salesOrderId: string,
): Promise<Record<string, unknown>> {
  const token = await bearerToken(page)
  const response = await page.request.get(
    `${API_BASE}/admin/sales-orders/${salesOrderId}`,
    { headers: { Authorization: `Bearer ${token}` } },
  )
  expect(response.ok(), `读取销售单 ${salesOrderId} 失败`).toBeTruthy()
  const body = (await response.json()) as { data?: Record<string, unknown> } & Record<
    string,
    unknown
  >
  return (body.data ?? body) as Record<string, unknown>
}

async function ensureDefaultDispatcher(browser: Browser): Promise<void> {
  const session = await openSession(browser, ADMIN)
  const { page, context } = session
  try {
    await page.goto('/master-data/procurement-responsibilities')
    await expect(page.getByRole('heading', { name: '采购责任规则' })).toBeVisible(VISIBLE)
    const rulesTable = page.locator('#procurement-responsibility-rules-table')
    if (
      !(await page.getByText('还没有采购责任规则').count()) &&
      (await rulesTable.getByText('默认调度人').count())
    ) {
      return
    }
    await page.locator('#procurement-responsibility-rules-create').click()
    const dialog = page.getByRole('dialog', { name: '新增采购责任规则' })
    await expect(dialog).toBeVisible(VISIBLE)
    await selectCombobox(page, dialog.getByLabel('规则类型'), '默认调度人')
    await selectCombobox(
      page,
      dialog.getByLabel('采购负责人'),
      new RegExp(`${PROCUREMENT.account}`),
    )
    await dialog.getByRole('button', { name: '保存规则' }).click()
    await expect(page.getByText('采购责任规则已新增')).toBeVisible(VISIBLE)
  } finally {
    await context.close()
  }
}

async function createCustomer(page: Page, legalName: string, creditCode: string): Promise<void> {
  await gotoNav(page, '客户中心', '/sales/customers')
  await page.getByRole('button', { name: '新建客户' }).click()
  const dialog = page.getByRole('dialog', { name: '新建客户' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.getByLabel('法定名称').fill(legalName)
  await dialog.getByLabel('客户简称').fill('流五客户')
  await dialog.getByLabel('统一社会信用代码').fill(creditCode)
  await dialog.getByRole('button', { name: '创建客户' }).click()
  await expect(page.getByText('客户已创建')).toBeVisible(VISIBLE)
  await expect(dialog).toBeHidden(VISIBLE)
}

async function startNewSalesOrder(page: Page): Promise<void> {
  await page.goto('/sales/orders?mode=create')
  await expect(page.getByRole('heading', { name: '单据头' })).toBeVisible(VISIBLE)
}

async function uploadContract(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date },
): Promise<void> {
  await page.locator('#sales-orders-create-contract-upload').click()
  const dialog = page.getByRole('dialog', { name: '上传合同 PDF' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.locator('#card-contracts-upload-pdf-input').setInputFiles(contractPdfPath())
  await dialog.getByLabel('合同编号').fill(input.contractNo)
  await selectCombobox(
    page,
    dialog.getByPlaceholder('搜索客户编号或名称'),
    new RegExp(input.customerName),
  )
  await selectCombobox(
    page,
    dialog.getByPlaceholder('搜索结算主体'),
    new RegExp(input.customerName),
  )
  await selectCombobox(page, dialog.getByLabel('付款条件'), '按合同约定')
  await pickIsoDate(page, 'card-contracts-upload-signed-at', isoDate(input.today))
  await pickIsoDate(page, 'card-contracts-upload-valid-from', isoDate(input.today))
  await pickIsoDate(page, 'card-contracts-upload-valid-to', isoDate(addDays(input.today, 7)))
  await dialog.getByRole('button', { name: '上传并归档' }).click()
  await expect(dialog).toBeHidden(VISIBLE)
  await expect(page.getByText(new RegExp(`客户\\s+${input.customerName}`))).toBeVisible(
    VISIBLE,
  )
}

async function pickSkuAndFillLine(page: Page, due: Date, quantity: string): Promise<void> {
  await page.getByRole('button', { name: '选择商品' }).click()
  const picker = page.getByRole('dialog', { name: '选择商品' })
  await expect(picker).toBeVisible(VISIBLE)
  await picker.getByPlaceholder('搜索 SKU、商品名称、编号或规格').fill(SKU_NAME)
  await picker.getByPlaceholder('搜索 SKU、商品名称、编号或规格').press('Enter')
  const skuRow = picker.getByRole('checkbox', { name: new RegExp(`选择\\s+${SKU_NAME}`) })
  await expect(skuRow).toBeVisible(VISIBLE)
  await skuRow.check()
  await picker.getByRole('button', { name: /加入所选/ }).click()
  await expect(picker).toBeHidden(VISIBLE)
  await expect(page.getByRole('button', { name: new RegExp(`更换销售项目 ${SKU_NAME}`) })).toBeVisible(
    VISIBLE,
  )
  await expect(page.locator('[data-testid^="sales-line-procurement-owner-"]')).toContainText(
    /采购/,
    VISIBLE,
  )
  await page.getByLabel('数量').first().fill(quantity)
  await pickIsoDate(page, 'sales-orders-create-batch-due-date', isoDate(due))
  await page.locator('#sales-orders-create-batch-due-date-apply').click()
  await expect(page.getByText('已批量设置交期')).toBeVisible(VISIBLE)
}

async function submitSalesOrder(page: Page, quantity: string): Promise<OrderSnapshot> {
  await page.locator('#sales-orders-create-submit').click()
  const confirm = page.getByRole('dialog', { name: '提交销售单' })
  await expect(confirm).toBeVisible(VISIBLE)
  await confirm.locator('#sales-orders-submit-confirm-confirm').click()
  await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, VISIBLE)
  const id = page.url().split('/').pop()?.split('?')[0] ?? ''
  expect(id.length).toBeGreaterThan(8)
  const detail = await fetchSalesOrder(page, id)
  const orderNo = String(detail.order_no ?? '')
  await expect(page.getByText('审批中').first()).toBeVisible(VISIBLE)
  await expect(page.getByText(orderNo).first()).toBeVisible(VISIBLE)
  const line = (
    (detail.working_copy as { lines?: Array<{ quantity?: string; unit_price_gross?: string }> } | undefined)
      ?.lines ??
    (
      (detail.submissions as Array<{ lines?: Array<{ quantity?: string; unit_price_gross?: string }> }>) ??
      []
    ).flatMap((item) => item.lines ?? [])
  )[0]
  return {
    id,
    orderNo,
    quantity: String(line?.quantity ?? quantity),
    unitPrice: String(line?.unit_price_gross ?? ''),
    submissionNo: submissionNoOf(detail),
  }
}

async function fillSalesHeader(page: Page): Promise<void> {
  await selectCombobox(page, page.getByLabel('福利场景'), '年节礼包')
}

async function attachContract(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date },
): Promise<void> {
  const combo = page.getByPlaceholder('搜索合同编号或客户')
  await combo.click()
  await combo.fill(input.contractNo)
  const existing = page.getByRole('option', { name: new RegExp(input.contractNo) })
  try {
    await expect(existing.first()).toBeVisible({ timeout: 8000 })
    await existing.first().click()
    await expect(page.getByText(new RegExp(`客户\\s+${input.customerName}`))).toBeVisible(
      VISIBLE,
    )
    return
  } catch {
    await page.keyboard.press('Escape')
  }
  await uploadContract(page, input)
}

async function createAndSubmitOrder(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date; due: Date; quantity: string },
): Promise<OrderSnapshot> {
  await startNewSalesOrder(page)
  await attachContract(page, input)
  await fillSalesHeader(page)
  await pickSkuAndFillLine(page, input.due, input.quantity)
  return submitSalesOrder(page, input.quantity)
}

async function openWorkspaceInbox(page: Page): Promise<void> {
  await page.goto('/workspace')
  await expect(page.getByRole('heading', { name: '我的工作台' })).toBeVisible(VISIBLE)
  await page.locator('#workspace-family-nav-approval').click()
}

async function openApprovalTask(page: Page, orderNo: string): Promise<void> {
  await openWorkspaceInbox(page)
  const search = page.locator('#workspace-queue-toolbar-search-input')
  await search.fill(orderNo)
  await search.press('Enter')
  const task = page.getByRole('button', { name: new RegExp(`销售单审批\\s+${orderNo}`) })
  await expect(task).toBeVisible(VISIBLE)
  await task.click()
  await expect(page.getByRole('heading', { name: new RegExp(orderNo) })).toBeVisible(VISIBLE)
}

async function decideOnWorkspace(
  page: Page,
  decision: '通过' | '驳回',
  reason?: string,
): Promise<void> {
  const dialogTitle = decision === '驳回' ? '确认驳回' : '确认通过'
  const confirmLabel = decision === '驳回' ? '确认驳回' : '确认通过'
  await page.getByRole('button', { name: decision, exact: true }).click()
  const dialog = page.getByRole('dialog', { name: dialogTitle })
  await expect(dialog).toBeVisible(VISIBLE)
  if (decision === '驳回') {
    await dialog.getByLabel('驳回原因').fill(reason ?? REJECT_REASON)
  }
  await dialog.getByRole('button', { name: confirmLabel }).click()
  await expect(dialog).toBeHidden(VISIBLE)
}

async function openSalesOrder(page: Page, order: OrderSnapshot): Promise<void> {
  await page.goto(`/sales/orders/${order.id}`)
  await expect(page.getByText(order.orderNo).first()).toBeVisible(VISIBLE)
}

async function assertRejectedNotEffective(page: Page, order: OrderSnapshot): Promise<void> {
  await openSalesOrder(page, order)
  await expect(page.getByText('审批中').first()).toBeVisible(VISIBLE)
  await expect(page.getByText('已生效')).toHaveCount(0)
  await expect(page.getByText('版本 尚未生效')).toBeVisible(VISIBLE)
  await page.getByRole('tab', { name: /^审批/ }).click()
  await expect(page.getByText('第 2 轮').first()).toBeVisible(VISIBLE)
  await expect(page.getByText('采购确认').first()).toBeVisible(VISIBLE)
  await expect(page.getByText('最近驳回')).toBeVisible(VISIBLE)
  await expect(page.getByText(REJECT_REASON)).toBeVisible(VISIBLE)
  const changeBtn = page.locator('#sales-orders-detail-start-change')
  await expect(changeBtn).toBeVisible(VISIBLE)
  await expect(changeBtn).toBeDisabled()
  await expect(changeBtn).toHaveAttribute(
    'title',
    '本单还在确认/审批中，请先处理完当前待办，再发起改单。',
  )
  await page.getByRole('tab', { name: /^概览/ }).click()
  await expect(page.getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
  await expect(page.getByText(new RegExp(`${order.quantity}\\s+盒`)).first()).toBeVisible(
    VISIBLE,
  )
  await page.getByRole('tab', { name: /^采购/ }).click()
  await expect(page.getByTestId('sales-order-purchase-status')).toContainText('采购单 0 笔')
  await expect(page.getByRole('link', { name: '继续分配供给' })).toHaveCount(0)
  const live = await fetchSalesOrder(page, order.id)
  expect(String(live.commercial_status ?? live.commercialStatus)).not.toBe('EFFECTIVE')
  expect(String(live.review_status ?? live.reviewStatus)).toMatch(/IN_APPROVAL/)
  expect(submissionNoOf(live)).toBe(order.submissionNo)
}

async function assertNoSupplyOrFulfillmentTask(page: Page, orderNo: string): Promise<void> {
  await page.goto('/workspace')
  const search = page.locator('#workspace-queue-toolbar-search-input')
  await search.fill(orderNo)
  await search.press('Enter')
  await expect(
    page.getByRole('button', { name: new RegExp(`待供给分配[\\s\\S]*${orderNo}|${orderNo}[\\s\\S]*待供给分配`) }),
  ).toHaveCount(0)
  await expect(
    page.getByRole('button', { name: new RegExp(`履约处理[\\s\\S]*${orderNo}`) }),
  ).toHaveCount(0)
}

async function withdrawApproval(page: Page, order: OrderSnapshot): Promise<void> {
  await openSalesOrder(page, order)
  await page.locator('#sales-orders-detail-cancel-approval-trigger').click()
  const dialog = page.getByRole('dialog', { name: '撤回审批' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.getByLabel('撤回原因').fill(WITHDRAW_REASON)
  await dialog.locator('#sales-orders-detail-cancel-approval-confirm').click()
  await expect(page.getByText('审批已撤回')).toBeVisible(VISIBLE)
  await expect(page.getByRole('heading', { name: '单据头' })).toBeVisible(VISIBLE)
  await expect(page.getByText('草稿').first()).toBeVisible(VISIBLE)
}

async function voidDraftViaHttp(page: Page, order: OrderSnapshot): Promise<void> {
  await expect(page.getByRole('button', { name: /作废/ })).toHaveCount(0)
  const detail = await fetchSalesOrder(page, order.id)
  const version = Number(detail.version ?? 1)
  const token = await bearerToken(page)
  const response = await page.request.post(
    `${API_BASE}/admin/sales-orders/${order.id}/void`,
    {
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
      data: { version },
    },
  )
  expect(response.ok(), `${VOID_HTTP_NOTE}; HTTP ${response.status()}`).toBeTruthy()
}

test.describe.configure({ mode: 'serial' })

test('销售单审批驳回后可照原条件承接、撤回改单重提或作废', async ({ browser }) => {
  test.setTimeout(12 * 60 * 1000)
  const today = new Date()
  const due = addDays(today, 7)
  const customerName = `流五驳回客户${Date.now().toString().slice(-6)}`
  const contractNo = `HT-F05-${Date.now().toString().slice(-8)}`
  const creditCode = uniqueCreditCode()

  await ensureDefaultDispatcher(browser)

  const sales = await openSession(browser, SALES)
  const procurement = await openSession(browser, PROCUREMENT)
  try {
    await test.step('销售创建客户', async () => {
      await createCustomer(sales.page, customerName, creditCode)
    })

    const orderA = await test.step('场景A 建单提交', async () =>
      createAndSubmitOrder(sales.page, {
        customerName,
        contractNo,
        today,
        due,
        quantity: '2',
      }))
    expect(orderA.submissionNo).toBeGreaterThan(0)

    await test.step('采购在采购确认节点驳回', async () => {
      await openApprovalTask(procurement.page, orderA.orderNo)
      await expect(procurement.page.getByText('第 1 轮').first()).toBeVisible(VISIBLE)
      await expect(procurement.page.getByText('采购确认').first()).toBeVisible(VISIBLE)
      await decideOnWorkspace(procurement.page, '驳回', REJECT_REASON)
    })

    await test.step('驳回后不生效、内容与 subject_version 不变、轮次加一、禁止变更单', async () => {
      await assertRejectedNotEffective(sales.page, orderA)
      await assertNoSupplyOrFulfillmentTask(procurement.page, orderA.orderNo)
      await openApprovalTask(procurement.page, orderA.orderNo)
      await expect(procurement.page.getByText('第 2 轮').first()).toBeVisible(VISIBLE)
      await expect(procurement.page.getByText('采购确认').first()).toBeVisible(VISIBLE)
      await expect(
        procurement.page.getByRole('button', { name: new RegExp(`待供给分配[\\s\\S]*${orderA.orderNo}`) }),
      ).toHaveCount(0)
    })

    await test.step('场景A 照原条件承接：不撤回不改单，新一轮通过后生效', async () => {
      await openApprovalTask(procurement.page, orderA.orderNo)
      await decideOnWorkspace(procurement.page, '通过')
      await openSalesOrder(sales.page, orderA)
      await expect(sales.page.getByText('已生效').first()).toBeVisible(VISIBLE)
      await expect(sales.page.getByText(/版本\s+v1/)).toBeVisible(VISIBLE)
      const live = await fetchSalesOrder(sales.page, orderA.id)
      expect(live.commercial_status ?? live.commercialStatus).toBe('EFFECTIVE')
      expect(submissionNoOf(live)).toBe(orderA.submissionNo)
    })

    const orderB = await test.step('场景B 建单提交并驳回', async () => {
      const created = await createAndSubmitOrder(sales.page, {
        customerName,
        contractNo,
        today,
        due,
        quantity: '2',
      })
      await openApprovalTask(procurement.page, created.orderNo)
      await decideOnWorkspace(procurement.page, '驳回', REJECT_REASON)
      await assertRejectedNotEffective(sales.page, created)
      return created
    })

    await test.step('场景B 撤回改单重提后审批通过生效', async () => {
      await withdrawApproval(sales.page, orderB)
      await expect(sales.page.getByRole('button', { name: '发起改单' })).toHaveCount(0)
      await sales.page.getByLabel('数量').first().fill('5')
      await sales.page.locator('#sales-orders-create-submit').click()
      const confirm = sales.page.getByRole('dialog', { name: '提交销售单' })
      await expect(confirm).toBeVisible(VISIBLE)
      await confirm.locator('#sales-orders-submit-confirm-confirm').click()
      await expect(sales.page).toHaveURL(new RegExp(`/sales/orders/${orderB.id}`), VISIBLE)
      const resubmitted = await fetchSalesOrder(sales.page, orderB.id)
      const newSubmissionNo = submissionNoOf(resubmitted)
      expect(newSubmissionNo).toBeGreaterThan(orderB.submissionNo)
      await expect(sales.page.getByText('审批中').first()).toBeVisible(VISIBLE)
      await openApprovalTask(procurement.page, orderB.orderNo)
      await decideOnWorkspace(procurement.page, '通过')
      await openSalesOrder(sales.page, orderB)
      await expect(sales.page.getByText('已生效').first()).toBeVisible(VISIBLE)
      await sales.page.getByRole('tab', { name: /^概览/ }).click()
      await expect(sales.page.getByText(/5\s+盒/)).toBeVisible(VISIBLE)
      const live = await fetchSalesOrder(sales.page, orderB.id)
      expect(live.commercial_status ?? live.commercialStatus).toBe('EFFECTIVE')
      expect(submissionNoOf(live)).toBe(newSubmissionNo)
    })

    const orderC = await test.step('场景C 建单提交并驳回', async () => {
      const created = await createAndSubmitOrder(sales.page, {
        customerName,
        contractNo,
        today,
        due,
        quantity: '2',
      })
      await openApprovalTask(procurement.page, created.orderNo)
      await decideOnWorkspace(procurement.page, '驳回', REJECT_REASON)
      await assertRejectedNotEffective(sales.page, created)
      await expect(sales.page.getByRole('button', { name: /作废/ })).toHaveCount(0)
      return created
    })

    await test.step('场景C 撤回后作废，主状态已作废', async () => {
      await withdrawApproval(sales.page, orderC)
      await voidDraftViaHttp(sales.page, orderC)
      await sales.page.reload()
      await expect(sales.page.getByText('已作废').first()).toBeVisible(VISIBLE)
      await expect(sales.page.getByText('本单已作废，不再进入履约或结案。')).toBeVisible(
        VISIBLE,
      )
      const live = await fetchSalesOrder(sales.page, orderC.id)
      expect(live.commercial_status ?? live.commercialStatus).toBe('VOIDED')
      const changeBtn = sales.page.locator('#sales-orders-detail-start-change')
      await expect(changeBtn).toBeVisible(VISIBLE)
      await expect(changeBtn).toBeDisabled()
      await expect(changeBtn).toHaveAttribute('title', /已作废|不能发起改单/)
      await assertNoSupplyOrFulfillmentTask(procurement.page, orderC.orderNo)
    })
  } finally {
    await sales.context.close()
    await procurement.context.close()
  }
})
