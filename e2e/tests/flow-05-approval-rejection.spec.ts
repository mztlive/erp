/**
 * 流程: [flow-05] 销售单审批驳回、修改原单重提与作废
 * 文档: docs/erp-phase-1.md §4.4、§7.3.1；approval-workflow-contract.md §4.4.2–§4.4.4、§11
 * 账号: xiaoshou（提交）→ caigou（采购确认节点驳回/通过）；admin 仅补采购责任默认调度人
 * 驳回结束原审批并退回提交人；按原内容或修改后重提均创建新审批实例。
 * 原审批及提交保留，未生效销售单不产生供给或履约任务。
 */
import { workPage } from "../helpers/workspace-page"
import { archiveContractViaUi, SIGNING_COMPANY } from "../helpers/contracts"
import { fillSalesLineUnitPrice, selectExistingContractForSalesOrder } from "../helpers/sales-orders"
import { fileURLToPath } from 'node:url'

import { test, expect, type Browser, type Locator, type Page } from '../helpers/test'

import { createCustomerViaUi } from '../helpers/customers'
import { openLoggedInWorkspace } from '../helpers/login'
import { ensureDefaultProcurementOwner } from '../helpers/procurement'
import {
  approveCurrentDocument,
  chooseOption,
  dismissToasts,
  expectToast,
  openWorkspaceTask,
  selectWorkspaceFamily,
  pickCalendarDay,
} from '../helpers/ui'

const VISIBLE = { timeout: 20_000 } as const
const API_BASE = process.env.API_BASE || 'http://127.0.0.1:10001'
const SKU_NAME = '狮峰明前龙井礼盒'
const UNIT_PRICE = '1288.00'
const REJECT_REASON = '无法履约，成本上涨，交期不满足'
const VOID_HTTP_NOTE = '销售单草稿作废无页面按钮，走已发布 POST /admin/sales-orders/{id}/void'

type OrderSnapshot = {
  id: string
  orderNo: string
  quantity: string
  unitPrice: string
  submissionNo: number
}

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
  const stamp = Date.now()
    .toString(36)
    .toUpperCase()
    .replace(/[^0-9A-Z]/g, '0')
  return `91110108MA01${stamp}`.slice(0, 18).padEnd(18, '0')
}

const CONTRACT_PDF = fileURLToPath(new URL('../fixtures/sample-contract.pdf', import.meta.url))

async function clickWithoutToastOverlay(
  page: Page,
  target: Locator,
  settled?: () => Promise<boolean>,
): Promise<void> {
  // Toast 可能在关闭后再次出现导致遮挡：循环关闭后短超时点按，成功即返回（与 flow-03/04 同款）。
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

async function fetchSalesOrder(page: Page, salesOrderId: string): Promise<Record<string, unknown>> {
  const token = await bearerToken(page)
  const response = await page.request.get(`${API_BASE}/admin/sales-orders/${salesOrderId}`, {
    headers: { Authorization: `Bearer ${token}` },
  })
  expect(response.ok(), `读取销售单 ${salesOrderId} 失败`).toBeTruthy()
  const body = (await response.json()) as { data?: Record<string, unknown> } & Record<
    string,
    unknown
  >
  return (body.data ?? body) as Record<string, unknown>
}

async function ensureDefaultDispatcher(browser: Browser): Promise<void> {
  const { page, context } = await openLoggedInWorkspace(browser, 'admin')
  try {
    await ensureDefaultProcurementOwner(page)
  } finally {
    await context.close()
  }
}

async function startNewSalesOrder(page: Page): Promise<void> {
  await page.goto('/sales/orders?mode=create')
  await expect(
    workPage(page)
      .getByRole('heading', { name: '新建销售单' })
      .or(workPage(page).getByRole('heading', { name: '业务信息' })),
  ).toBeVisible(VISIBLE)
  await expect(workPage(page).locator('#sales-orders-create-entry-upload')).toBeVisible(VISIBLE)
}

async function uploadContract(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date },
): Promise<void> {
  await archiveContractViaUi(page, { contractNo: input.contractNo, customerName: input.customerName, pdf: CONTRACT_PDF, validTo: isoDate(addDays(input.today, 7)) })
  await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(new RegExp(input.customerName))
}

async function pickSkuAndFillLine(page: Page, due: Date, quantity: string): Promise<void> {
  await workPage(page).locator('#sales-orders-create-line-items-add').click()
  const picker = workPage(page).getByRole('dialog', { name: '添加商品' })
  await expect(picker).toBeVisible(VISIBLE)
  await picker.locator("#sales-orders-sku-picker-filter-search").fill(SKU_NAME)
  await picker.locator("#sales-orders-sku-picker-filter-search").press('Enter')
  const skuRow = picker.getByRole('checkbox', { name: new RegExp(`选择\\s+${SKU_NAME}`) })
  await expect(skuRow).toBeVisible(VISIBLE)
  await skuRow.check()
  await picker.getByRole('button', { name: /加入所选/ }).click()
  await expect(picker).toBeHidden(VISIBLE)
  await expect(
    workPage(page).getByRole('button', { name: new RegExp(`更换销售项目 ${SKU_NAME}`) }),
  ).toBeVisible(VISIBLE)
  await expect(workPage(page).locator('[data-testid^="sales-line-procurement-owner-"]')).toContainText(
    /陈国平|采购/,
    VISIBLE,
  )
  await workPage(page).getByLabel('数量').first().fill(quantity)
  await fillSalesLineUnitPrice(page, UNIT_PRICE)
  await workPage(page).locator('#sales-orders-create-batch-due-date-open').click()
  await pickCalendarDay(page, workPage(page).locator('#sales-orders-create-batch-due-date'), isoDate(due))
  await workPage(page).locator('#sales-orders-create-batch-due-date-apply').click()
  await expectToast(page, '已批量设置交期')
}

async function submitSalesOrder(page: Page, quantity: string): Promise<OrderSnapshot> {
  await clickWithoutToastOverlay(page, workPage(page).locator('#sales-orders-create-submit'), async () =>
    workPage(page)
      .getByRole('dialog', { name: '提交销售单' })
      .isVisible()
      .catch(() => false),
  )
  const confirm = workPage(page).getByRole('dialog', { name: '提交销售单' })
  await expect(confirm).toBeVisible(VISIBLE)
  // 批量交期 toast 常盖住确认按钮：先清 toast 再点，盖住不散时走 DOM 派发。
  // settled 只做即时判断：第一次点按前页面必然还在新建页，等 URL 只会白等。
  await clickWithoutToastOverlay(
    page,
    confirm.locator('#sales-orders-submit-confirm-confirm'),
    async () => !/\/sales\/orders\?mode=create/.test(page.url()),
  )
  await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, VISIBLE)
  const id = page.url().split('/').pop()?.split('?')[0] ?? ''
  expect(id.length).toBeGreaterThan(8)
  const detail = await fetchSalesOrder(page, id)
  const orderNo = String(detail.order_no ?? '')
  await expect(workPage(page).getByText('审批中').first()).toBeVisible(VISIBLE)
  await expect(workPage(page).getByText(orderNo).first()).toBeVisible(VISIBLE)
  const line = ((
    detail.working_copy as
      | { lines?: Array<{ quantity?: string; unit_price_gross?: string }> }
      | undefined
  )?.lines ??
    (
      (detail.submissions as Array<{
        lines?: Array<{ quantity?: string; unit_price_gross?: string }>
      }>) ?? []
    ).flatMap((item) => item.lines ?? []))[0]
  return {
    id,
    orderNo,
    quantity: String(line?.quantity ?? quantity),
    unitPrice: String(line?.unit_price_gross ?? ''),
    submissionNo: submissionNoOf(detail),
  }
}

async function fillSalesHeader(page: Page): Promise<void> {
  await chooseOption(page, workPage(page).getByLabel('福利场景'), '礼包')
}

async function attachContract(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date; newContract?: boolean },
): Promise<void> {
  if (input.newContract) {
    await uploadContract(page, input)
    return
  }
  await selectExistingContractForSalesOrder(page, input.contractNo)
  await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(input.customerName)
}

async function createAndSubmitOrder(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date; due: Date; quantity: string; newContract?: boolean },
): Promise<OrderSnapshot> {
  await startNewSalesOrder(page)
  await attachContract(page, input)
  await fillSalesHeader(page)
  await pickSkuAndFillLine(page, input.due, input.quantity)
  return submitSalesOrder(page, input.quantity)
}

async function openApprovalTask(page: Page, orderNo: string): Promise<void> {
  await openWorkspaceTask(page, '销售单审批', orderNo, 'approval')
}

async function decideOnWorkspace(
  page: Page,
  decision: '通过' | '驳回',
  reason?: string,
): Promise<void> {
  if (decision === '通过') {
    await approveCurrentDocument(page)
    return
  }
  await workPage(page).getByRole('button', { name: /^驳回$/ }).click()
  const dialog = workPage(page).getByRole('dialog', { name: '确认驳回' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.getByLabel('驳回原因').fill(reason ?? REJECT_REASON)
  const [response] = await Promise.all([
    page.waitForResponse((reply) =>
      reply.request().method() === 'POST' && new URL(reply.url()).pathname === '/admin/approval-decisions',
    ),
    dialog.getByRole('button', { name: '确认驳回' }).click(),
  ])
  const body = await response.json() as { success: boolean; errorMessage?: string }
  expect(response.ok() && body.success, `驳回失败（HTTP ${response.status()}）：${body.errorMessage ?? ''}`).toBe(true)
  await expect(dialog).toBeHidden(VISIBLE)
}

async function openSalesOrder(page: Page, order: OrderSnapshot): Promise<void> {
  await page.goto(`/sales/orders/${order.id}`)
  await expect(workPage(page).getByText(order.orderNo).first()).toBeVisible(VISIBLE)
}

async function expectSalesQuantity(page: Page, quantity: string): Promise<void> {
  await workPage(page).getByRole('tab', { name: /^交付与验收/ }).click()
  const row = workPage(page).getByRole('region', { name: '销售明细与交付' }).getByRole('row').filter({ hasText: SKU_NAME })
  await expect(row).toHaveCount(1)
  await expect(row.getByRole('cell').nth(1)).toHaveText(
    new RegExp(`^${Number(quantity)}(?:\\.0+)?\\s*盒$`),
  )
}

function approvalInstance(detail: Record<string, unknown>) {
  return (detail.approval as {
    instance?: { id: string; status: string; latest_rejection: string | null; current_round_no: number }
  } | undefined)?.instance
}

async function expectRejectedOrder(page: Page, order: OrderSnapshot) {
  const detail = await fetchSalesOrder(page, order.id)
  expect(detail).toMatchObject({
    id: order.id,
    order_no: order.orderNo,
    commercial_status: 'DRAFT',
    review_status: 'REJECTED',
    current_revision_id: null,
    purchase_order_count: 0,
    purchase_creation_access: { allowed: false, task_count: 0 },
  })
  expect(submissionNoOf(detail)).toBe(order.submissionNo)
  const instance = approvalInstance(detail)
  expect(instance).toMatchObject({
    status: 'REJECTED', latest_rejection: REJECT_REASON, current_round_no: 1,
  })
  return { detail, instanceId: instance!.id }
}

async function openRejectedOrderTask(page: Page, order: OrderSnapshot): Promise<void> {
  await openWorkspaceTask(page, '驳回待处理', order.orderNo, 'approval')
  await expect(workPage(page).getByText('本次审批已驳回', { exact: true })).toBeVisible(VISIBLE)
  await expect(workPage(page).getByText(REJECT_REASON, { exact: true }).first()).toBeVisible(VISIBLE)
  await expect(workPage(page).getByRole('button', { name: /^(通过|通过审批|同意审批|发起改单)$/ })).toHaveCount(0)
}

async function enterRejectedOrderEditor(page: Page, order: OrderSnapshot): Promise<void> {
  await openRejectedOrderTask(page, order)
  await workPage(page).getByRole('button', { name: '修改或重新提交', exact: true }).click()
  await expect(page).toHaveURL(new RegExp(`/sales/orders/${order.id}(?:\\?|$)`), VISIBLE)
  await expect(workPage(page).getByText('已驳回', { exact: true }).first()).toBeVisible(VISIBLE)
  await expect(workPage(page).getByRole('heading', { name: '销售明细', exact: true })).toBeVisible(VISIBLE)
}

async function resubmitRejectedOrder(page: Page, order: OrderSnapshot, reason: string) {
  const submit = workPage(page).locator('#sales-orders-create-submit')
  await expect(submit).toBeEnabled(VISIBLE)
  await dismissToasts(page)
  await submit.click()
  const dialog = workPage(page).getByRole('dialog', { name: '提交销售单' })
  const confirm = dialog.locator('#sales-orders-submit-confirm-confirm')
  await expect(confirm).toBeDisabled(VISIBLE)
  await dialog.getByLabel('重新提交说明').fill(reason)
  const [response] = await Promise.all([
    page.waitForResponse((reply) => reply.request().method() === 'POST' &&
      new URL(reply.url()).pathname === `/admin/sales-orders/${order.id}/submit`,
    ),
    confirm.click(),
  ])
  const payload = await response.text()
  expect(response.ok(), payload).toBe(true)
  expect(JSON.parse(payload).success, payload).toBe(true)
  await expect(workPage(page).getByText('审批中', { exact: true }).first()).toBeVisible(VISIBLE)
  return fetchSalesOrder(page, order.id)
}

async function expectNoQueueTask(page: Page, family: string, orderNo: string, label: RegExp): Promise<void> {
  await page.goto(`/workspace/tasks`)
  await selectWorkspaceFamily(page, family)
  const queue = workPage(page).getByRole('region', { name: '任务队列', exact: true })
  const rows = queue.locator('[id^="workspace-task-table-row-"]')
  const empty = queue.getByText(/没有符合条件的事项|当前没有待处理事项/)
  await expect(rows.first().or(empty).first()).toBeVisible(VISIBLE)
  await expect(queue.getByRole('row', { name: label }).and(
    queue.getByRole('row', { name: new RegExp(orderNo) }),
  )).toHaveCount(0)
}

async function assertNoSupplyOrFulfillmentTask(page: Page, orderNo: string): Promise<void> {
  await expectNoQueueTask(page, 'procurement', orderNo, /待安排采购|供给分配/)
  await expectNoQueueTask(page, 'fulfillment', orderNo, /履约处理/)
}

async function voidDraftViaHttp(page: Page, order: OrderSnapshot): Promise<void> {
  await expect(workPage(page).getByRole('button', { name: /作废/ })).toHaveCount(0)
  const detail = await fetchSalesOrder(page, order.id)
  const version = Number(detail.version ?? 1)
  const token = await bearerToken(page)
  const response = await page.request.post(`${API_BASE}/admin/sales-orders/${order.id}/void`, {
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    data: { version },
  })
  expect(response.ok(), `${VOID_HTTP_NOTE}; HTTP ${response.status()}`).toBeTruthy()
}

function expectResubmission(
  detail: Record<string, unknown>, order: OrderSnapshot, oldInstanceId: string, quantity: string,
): void {
  expect(detail).toMatchObject({ id: order.id, order_no: order.orderNo, review_status: 'IN_APPROVAL' })
  const instance = approvalInstance(detail)
  expect(instance).toMatchObject({ status: 'RUNNING', current_round_no: 1 })
  expect(instance?.id).not.toBe(oldInstanceId)
  const submissions = detail.submissions as Array<{
    submission_no: number
    status: string
    lines: Array<{ quantity: string; unit_price_gross: string }>
  }>
  const old = submissions.find((item) => item.submission_no === order.submissionNo)
  const latest = submissions.find((item) => item.submission_no === submissionNoOf(detail))
  expect(old?.status).toBe('REJECTED')
  expect(old?.lines[0]?.quantity).toBe(order.quantity)
  expect(old?.lines[0]?.unit_price_gross).toBe(order.unitPrice)
  expect(latest?.lines[0]?.unit_price_gross).toBe(order.unitPrice)
  expect(Number(latest?.lines[0]?.quantity)).toBe(Number(quantity))
  expect(submissionNoOf(detail)).toBeGreaterThan(order.submissionNo)
}

test.describe.configure({ mode: 'serial' })

test('销售单终态驳回后可按原内容重提、修改后重提或作废', async ({ browser }) => {
  test.setTimeout(12 * 60 * 1000)
  const today = new Date()
  const due = addDays(today, 7)
  const customerName = `流五驳回客户${Date.now().toString().slice(-6)}`
  const contractNo = `HT-F05-${Date.now().toString().slice(-8)}`
  const creditCode = uniqueCreditCode()

  await ensureDefaultDispatcher(browser)

  const sales = await openLoggedInWorkspace(browser, 'xiaoshou')
  const procurement = await openLoggedInWorkspace(browser, 'caigou')
  try {
    await test.step('销售创建客户', async () => {
      await createCustomerViaUi(sales.page, {
        legalName: customerName,
        shortName: '流五客户',
        creditCode,
        paymentTermLabel: '货到 15 天',
        contact: { name: '李测', phone: '13800138001' },
        address: '北京市朝阳区测试路 1 号',
      })
    })

    const orderA = await test.step('场景A 建单提交', async () =>
      createAndSubmitOrder(sales.page, {
        newContract: true,
        customerName,
        contractNo,
        today,
        due,
        quantity: '2',
      }))
    expect(orderA.submissionNo).toBeGreaterThan(0)

    await test.step('场景 A：终态驳回后，提交人说明依据并按原内容重提', async () => {
      await openApprovalTask(procurement.page, orderA.orderNo)
      await expect(workPage(procurement.page).getByText('第 1 轮').first()).toBeVisible(VISIBLE)
      await expect(workPage(procurement.page).getByText('采购确认').first()).toBeVisible(VISIBLE)
      await decideOnWorkspace(procurement.page, '驳回')
      const rejected = await expectRejectedOrder(sales.page, orderA)
      await assertNoSupplyOrFulfillmentTask(procurement.page, orderA.orderNo)
      await expectNoQueueTask(procurement.page, 'approval', orderA.orderNo, /销售单审批/)

      await enterRejectedOrderEditor(sales.page, orderA)
      const resubmitted = await resubmitRejectedOrder(sales.page, orderA, '已核实供给，按原内容重新提交')
      expectResubmission(resubmitted, orderA, rejected.instanceId, orderA.quantity)
      await openApprovalTask(procurement.page, orderA.orderNo)
      await decideOnWorkspace(procurement.page, '通过')
      await openSalesOrder(sales.page, orderA)
      await expect(workPage(sales.page).getByText('已生效').first()).toBeVisible(VISIBLE)
      expect((await fetchSalesOrder(sales.page, orderA.id)).commercial_status).toBe('EFFECTIVE')
    })

    await test.step('场景 B：保留同一单号及旧提交，修改数量后重提并生效', async () => {
      const order = await createAndSubmitOrder(sales.page, {
        customerName, contractNo, today, due, quantity: '2',
      })
      await openApprovalTask(procurement.page, order.orderNo)
      await decideOnWorkspace(procurement.page, '驳回')
      const rejected = await expectRejectedOrder(sales.page, order)
      await enterRejectedOrderEditor(sales.page, order)
      await expect(workPage(sales.page).getByText(`合同编号：${contractNo} · v1`, { exact: true })).toBeVisible(VISIBLE)
      await expect(workPage(sales.page).locator('#sales-orders-create-customer')).toContainText(customerName, VISIBLE)
      await expect(workPage(sales.page).locator('#sales-orders-create-settlement')).toHaveValue(new RegExp(SIGNING_COMPANY), VISIBLE)
      await workPage(sales.page).getByLabel('数量').first().fill('5')
      const resubmitted = await resubmitRejectedOrder(sales.page, order, '按客户确认后的数量重新提交')
      expectResubmission(resubmitted, order, rejected.instanceId, '5')
      const previous = (resubmitted.submissions as Array<{ submission_no: number }>)
        .filter((item) => item.submission_no === order.submissionNo)
      expect(previous).toEqual((rejected.detail.submissions as Array<{ submission_no: number }>)
        .filter((item) => item.submission_no === order.submissionNo))
      await openApprovalTask(procurement.page, order.orderNo)
      await decideOnWorkspace(procurement.page, '通过')
      await openSalesOrder(sales.page, order)
      await expect(workPage(sales.page).getByText('已生效').first()).toBeVisible(VISIBLE)
      await expectSalesQuantity(sales.page, '5')
      const live = await fetchSalesOrder(sales.page, order.id)
      expect(live.commercial_status).toBe('EFFECTIVE')
      expect(submissionNoOf(live)).toBe(submissionNoOf(resubmitted))
    })

    await test.step('场景 C：作废终态驳回的销售单，关闭提交人的驳回待办', async () => {
      const order = await createAndSubmitOrder(sales.page, {
        customerName, contractNo, today, due, quantity: '2',
      })
      await openApprovalTask(procurement.page, order.orderNo)
      await decideOnWorkspace(procurement.page, '驳回')
      const rejected = await expectRejectedOrder(sales.page, order)
      await enterRejectedOrderEditor(sales.page, order)
      await voidDraftViaHttp(sales.page, order)
      await sales.page.reload()
      await expect(workPage(sales.page).getByText('已作废', { exact: true }).first()).toBeVisible(VISIBLE)
      const live = await fetchSalesOrder(sales.page, order.id)
      expect(live.commercial_status).toBe('VOIDED')
      expect(approvalInstance(live)).toMatchObject({ id: rejected.instanceId, status: 'REJECTED' })
      await assertNoSupplyOrFulfillmentTask(procurement.page, order.orderNo)
      await expectNoQueueTask(sales.page, 'approval', order.orderNo, /驳回待处理/)
    })
  } finally {
    await sales.context.close()
    await procurement.context.close()
  }
})
