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

import { test, expect, type Browser, type Locator, type Page } from '@playwright/test'

import { createCustomerViaUi } from '../helpers/customers'
import { openLoggedInWorkspace } from '../helpers/login'
import { ensureDefaultProcurementOwner } from '../helpers/procurement'
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
} from '../helpers/ui'

const VISIBLE = { timeout: 20_000 } as const
const API_BASE = process.env.API_BASE || 'http://127.0.0.1:10001'
const SKU_NAME = '狮峰明前龙井礼盒'
const REJECT_REASON = '无法履约，成本上涨，交期不满足'
const WITHDRAW_REASON = '与客户改数量后重提'
const VOID_HTTP_NOTE =
  '销售单草稿作废无页面按钮，走已发布 POST /admin/sales-orders/{id}/void'

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
    page
      .getByRole('heading', { name: '新建销售单' })
      .or(page.getByRole('heading', { name: '业务信息' })),
  ).toBeVisible(VISIBLE)
  await expect(page.locator('#sales-orders-create-contract')).toBeVisible(VISIBLE)
}

async function uploadContract(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date },
): Promise<void> {
  // 同 id 有 button 与 div 两个元素：用角色定位按钮。
  await page.getByRole('button', { name: '上传合同 PDF' }).click()
  const dialog = page.getByRole('dialog', { name: '上传合同 PDF' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.locator('#card-contracts-upload-pdf-input').setInputFiles(contractPdfPath())
  await dialog.getByLabel('合同编号').fill(input.contractNo)
  await chooseOption(
    page,
    dialog.getByPlaceholder('搜索客户编号或名称'),
    new RegExp(input.customerName),
    input.customerName,
  )
  await chooseOption(
    page,
    dialog.getByPlaceholder('搜索结算主体'),
    new RegExp(input.customerName),
    input.customerName,
  )
  await chooseOption(page, dialog.getByLabel('付款条件'), '按合同约定')
  await pickCalendarDay(page, page.locator('#card-contracts-upload-signed-at'), isoDate(input.today))
  await pickCalendarDay(page, page.locator('#card-contracts-upload-valid-from'), isoDate(input.today))
  await pickCalendarDay(
    page,
    page.locator('#card-contracts-upload-valid-to'),
    isoDate(addDays(input.today, 7)),
  )
  await dialog.getByRole('button', { name: '上传并归档' }).click()
  await expect(dialog).toBeHidden(VISIBLE)
  await expect(page.getByText(new RegExp(`客户\\s+${input.customerName}`))).toBeVisible(
    VISIBLE,
  )
}

async function pickSkuAndFillLine(page: Page, due: Date, quantity: string): Promise<void> {
  await page.locator("#sales-orders-create-line-items-add").click()
  const picker = page.getByRole('dialog', { name: '添加商品' })
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
  await page.locator("#sales-orders-create-batch-due-date-open").click()
  await pickCalendarDay(page, page.locator('#sales-orders-create-batch-due-date'), isoDate(due))
  await page.locator('#sales-orders-create-batch-due-date-apply').click()
  await expectToast(page, '已批量设置交期')
}

async function submitSalesOrder(page: Page, quantity: string): Promise<OrderSnapshot> {
  await clickWithoutToastOverlay(page, page.locator('#sales-orders-create-submit'), async () =>
    page.getByRole('dialog', { name: '提交销售单' }).isVisible().catch(() => false),
  )
  const confirm = page.getByRole('dialog', { name: '提交销售单' })
  await expect(confirm).toBeVisible(VISIBLE)
  // 批量交期 toast 常盖住确认按钮：先清 toast 再点，盖住不散时走 DOM 派发。
  await clickWithoutToastOverlay(page, confirm.locator('#sales-orders-submit-confirm-confirm'), async () => {
    await page.waitForURL(/\/sales\/orders\/[^/?]+/, { timeout: 2_000 }).catch(() => undefined)
    return !/\/sales\/orders\?mode=create/.test(page.url())
  })
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
  await chooseOption(page, page.getByLabel('福利场景'), '年节礼包')
}

async function attachContract(
  page: Page,
  input: { customerName: string; contractNo: string; today: Date },
): Promise<void> {
  try {
    await chooseOption(
      page,
      page.getByPlaceholder('搜索合同编号或客户'),
      new RegExp(input.contractNo),
      input.contractNo,
    )
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

async function openApprovalTask(page: Page, orderNo: string): Promise<void> {
  await openWorkspaceTask(page, '销售单审批', orderNo, 'approval')
  await expect(page.getByText(orderNo).first()).toBeVisible(VISIBLE)
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
  await page.getByRole('button', { name: /^驳回$/ }).click()
  const dialog = page.getByRole('dialog', { name: '确认驳回' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.getByLabel('驳回原因').fill(reason ?? REJECT_REASON)
  await dialog.getByRole('button', { name: '确认驳回' }).click()
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
  await expect(page.getByText(/尚未生效|审批中/).first()).toBeVisible(VISIBLE)
  await page.getByRole('tab', { name: /^审批/ }).click()
  await expect(page.getByText('第 2 轮').first()).toBeVisible(VISIBLE)
  await expect(page.getByText('采购确认').first()).toBeVisible(VISIBLE)
  await expect(page.getByText('最近驳回')).toBeVisible(VISIBLE)
  await expect(page.getByText(REJECT_REASON).first()).toBeVisible(VISIBLE)
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
  await expect(page.getByTestId('sales-order-purchase-status')).toContainText('待采购')
  await expect(page.getByRole('link', { name: '继续分配供给' })).toHaveCount(0)
  const live = await fetchSalesOrder(page, order.id)
  expect(String(live.commercial_status ?? live.commercialStatus)).not.toBe('EFFECTIVE')
  expect(String(live.review_status ?? live.reviewStatus)).toMatch(/IN_APPROVAL/)
  expect(submissionNoOf(live)).toBe(order.submissionNo)
}

async function assertNoSupplyOrFulfillmentTask(page: Page, orderNo: string): Promise<void> {
  await page.goto('/workspace')
  // 后端搜索不匹配单号，填单号会把列表滤空导致断言恒成立；直接断言无匹配任务。
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
  // 撤回框实现为 alertdialog 而非 dialog。
  const dialog = page.getByRole('alertdialog', { name: '撤回审批' })
  await expect(dialog).toBeVisible(VISIBLE)
  await dialog.getByLabel('撤回原因').fill(WITHDRAW_REASON)
  await dialog.locator('#sales-orders-detail-cancel-approval-confirm').click()
  // 成功 toast 仅展示数秒，点按超时（按钮随框卸载）后再断言必错过：以框关闭 + 回到草稿为准。
  await expect(dialog).toBeHidden(VISIBLE)
  await expect(page.getByText('审批已撤回').first()).toBeVisible({ timeout: 5_000 }).catch(() => undefined)
  await expect(
    page
      .getByRole('heading', { name: '编辑销售单' })
      .or(page.getByRole('heading', { name: '业务信息' })),
  ).toBeVisible(VISIBLE)
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
      // 改数后工作副本自动保存完成前提交保持禁用：等提交可用再点，避免确认框空关。
      await expect(sales.page.locator('#sales-orders-create-submit')).toBeEnabled(VISIBLE)
      await clickWithoutToastOverlay(sales.page, sales.page.locator('#sales-orders-create-submit'), async () =>
        sales.page.getByRole('dialog', { name: '提交销售单' }).isVisible().catch(() => false),
      )
      const confirm = sales.page.getByRole('dialog', { name: '提交销售单' })
      await expect(confirm).toBeVisible(VISIBLE)
      // 重提时页面本来就在详情页，不能用 URL 是否含单据 id 判定（恒为真导致跳过点按）：
      // 改以确认框关闭为准，后续轮次号与审批中状态再验证提交确实发生。
      await expect(confirm.locator('#sales-orders-submit-confirm-confirm')).toBeEnabled(VISIBLE)
      const resubmitting = sales.page
        .waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            response.url().includes(`/sales-orders/${orderB.id}/submit`),
          { timeout: 20_000 },
        )
        .catch(() => undefined)
      await clickWithoutToastOverlay(
        sales.page,
        confirm.locator('#sales-orders-submit-confirm-confirm'),
        async () => !(await confirm.isVisible().catch(() => true)),
      )
      const resubmitResponse = await resubmitting
      expect(resubmitResponse?.ok()).toBe(true)
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
