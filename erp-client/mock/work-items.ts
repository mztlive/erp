import type { StatusTone } from "@/components/ui/status-badge"
import { actionLabels } from "@/lib/ui-text"
import type { ProcurementConfirmationTask } from "@/features/procurement-confirmation/types"
import { PROCUREMENT_CONFIRMATION_SEED } from "@/mock/procurement-confirmation"

/**
 * W02 domain fixtures — formal work_item projections for the unified queue.
 * Not a server; shapes mirror docs/ui-workspaces/w02-unified-task-queue.md §8.
 */

export type WorkItemFamily =
  | "approval"
  | "finance"
  | "fulfillment"
  | "exception"
  | "procurement"

export type WorkItemStatusCode =
  | "UNCLAIMED"
  | "PENDING"
  | "IN_PROGRESS"
  | "COMPLETED"
  | "TRANSFERRED"
  | "CLOSED"

export type WorkItemActionCode =
  | "CLAIM"
  | "DEFER"
  | "SAVE_EVIDENCE"
  | "QUERY_RESULT"
  | "TRANSFER"
  | "CLOSE"
  | "COMPLETE"

export type WorkItemFixture = Readonly<{
  id: string
  workItemType: string
  workItemTypeLabel: string
  family: WorkItemFamily
  /** Registered handler key; must exist in front-end map. */
  handlerKey: string
  /** Deep-link into specialized workspace when present. */
  handlerHref?: string
  /** Only formal completion action identity for this type. */
  completionAction: string
  businessObject: string
  counterparty: string
  enteredAt: string
  enteredDateTime: string
  dueAt: string
  dueDateTime: string
  responsibleParty: string
  reason: string
  impact: string
  /** Sensitive projection; cleared on permission revoke. */
  impactSensitive?: string
  statusCode: WorkItemStatusCode
  status: { label: string; tone: StatusTone }
  priority: number
  priorityLabel: string
  subjectVersion: string
  allowedActions: readonly WorkItemActionCode[]
  actionBlockers?: Readonly<Partial<Record<WorkItemActionCode, string>>>
  /**
   * Close is only allowed for server-confirmed misroute/duplicate/replacement.
   * Approval / confirm / result-unknown / compensation: always false.
   */
  closeAllowed: boolean
  scopeTags: readonly string[]
  summaryFields: readonly { label: string; value: string; numeric?: boolean }[]
  checkItems?: readonly string[]
  actionLabel?: string
  /** Compatible continuous-process group (same processor family). */
  processorGroup: string
}>

const money = (n: number) =>
  new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
  }).format(n)

/** 与 W07 队列共用的投影参照时钟（mock 世界统一为 2026-08-01 10:00）。 */
const QUEUE_REFERENCE_CLOCK = new Date("2026-08-01T10:00:00+08:00")

function sameCalendarDay(left: Date, right: Date): boolean {
  return (
    left.getFullYear() === right.getFullYear() &&
    left.getMonth() === right.getMonth() &&
    left.getDate() === right.getDate()
  )
}

function dueMetaFromIso(iso: string): { label: string; overdue: boolean } {
  const due = new Date(iso)
  const reference = QUEUE_REFERENCE_CLOCK
  const time = `${String(due.getHours()).padStart(2, "0")}:${String(due.getMinutes()).padStart(2, "0")}`
  if (due.getTime() < reference.getTime()) {
    return { label: "已超期", overdue: true }
  }
  if (sameCalendarDay(due, reference)) {
    return { label: `今天 ${time}`, overdue: false }
  }
  return {
    label: `${due.getMonth() + 1} 月 ${due.getDate()} 日 ${time}`,
    overdue: false,
  }
}

function enteredMetaFromSubmitted(submittedAt: string): {
  label: string
  iso: string
} {
  const [datePart, timePart] = submittedAt.split(" ")
  const iso = `${datePart}T${timePart}:00+08:00`
  const at = new Date(iso)
  const reference = QUEUE_REFERENCE_CLOCK
  const dayDiff = Math.round(
    (reference.getTime() - at.getTime()) / 86_400_000
  )
  const label =
    dayDiff <= 0
      ? `今天 ${timePart}`
      : dayDiff === 1
        ? `昨天 ${timePart}`
        : `${at.getMonth() + 1} 月 ${at.getDate()} 日 ${timePart}`
  return { label, iso }
}

function priorityMetaFromSeed(priority: number): {
  priority: number
  priorityLabel: string
} {
  if (priority >= 30) return { priority: 1, priorityLabel: "紧急" }
  if (priority >= 20) return { priority: 2, priorityLabel: "高" }
  return { priority: 3, priorityLabel: "普通" }
}

const CONFIRM_ORIGIN_REASON: Record<
  ProcurementConfirmationTask["salesSubmission"]["origin"],
  string
> = {
  INITIAL: "销售单已提交，供应商与成本信息待确认",
  CHANGED_TERMS_AFTER_REJECTION: "改品改价后重新提交，须再次确认",
  LOW_MARGIN_MANAGER_APPROVED: "低毛利已获上级确认，仍待采购确认",
}

const CONFIRM_CHECK_ITEM_BY_ISSUE: Record<string, string> = {
  QTY_COVERAGE_INCOMPLETE: "成本覆盖全部明细",
  QUALIFICATION_INVALID: "供应商资质在有效期内",
  DELIVERY_LATER_THAN_COMMITMENT: "交付方式与客户要求一致",
}

/** W02 采购确认族：直接由 W07 二次确认队列的待处理项投影生成。 */
function buildProcurementFamilyFixture(
  task: ProcurementConfirmationTask
): WorkItemFixture {
  const mine = task.responsibilityScope === "mine"
  const due = dueMetaFromIso(task.dueAt)
  const entered = enteredMetaFromSubmitted(task.salesSubmission.submittedAt)
  const priorityMeta = priorityMetaFromSeed(task.priority)
  const statusCode: WorkItemStatusCode = due.overdue
    ? "PENDING"
    : task.status === "IN_PROGRESS"
      ? "IN_PROGRESS"
      : "PENDING"
  const status =
    due.overdue
      ? { label: "已超期", tone: "destructive" as const }
      : statusCode === "IN_PROGRESS"
        ? { label: "处理中", tone: "info" as const }
        : { label: "待处理", tone: "warning" as const }
  const params = new URLSearchParams({
    from: "W02",
    currentWorkItemId: task.workItemId,
  })
  if (!mine) params.set("scope", "role_pool")
  const checkItems = [
    ...task.decisionSummary.blockingIssues,
    ...task.decisionSummary.warnings,
  ]
    .map((issue) => CONFIRM_CHECK_ITEM_BY_ISSUE[issue.code])
    .filter(
      (label, index, labels): label is string =>
        label !== undefined && labels.indexOf(label) === index
    )
  return {
    id: task.workItemId,
    workItemType: "PROCUREMENT_SECOND_CONFIRM",
    workItemTypeLabel: "采购二次确认",
    family: "procurement",
    handlerKey: "procurement_second_confirm",
    handlerHref: `/procurement/confirm?${params.toString()}`,
    completionAction: "CONFIRM_PROCUREMENT_PLAN",
    businessObject: `销售单 · ${task.salesSubmission.salesOrderNo}`,
    counterparty: task.salesSubmission.customerSnapshot,
    enteredAt: entered.label,
    enteredDateTime: entered.iso,
    dueAt: due.label,
    dueDateTime: task.dueAt,
    responsibleParty: mine ? "采购部 · 李采购" : "采购部 · 待领取",
    reason: CONFIRM_ORIGIN_REASON[task.salesSubmission.origin],
    impact: task.impactSummary,
    impactSensitive: "预估成本 " + money(Number(task.decisionSummary.estimatedPurchaseGross)),
    statusCode,
    status,
    priority: priorityMeta.priority,
    priorityLabel: priorityMeta.priorityLabel,
    subjectVersion: task.subjectVersion,
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "COMPLETE", "TRANSFER"],
    closeAllowed: false,
    scopeTags: mine ? ["我的待办", "团队"] : ["待领取", "团队"],
    processorGroup: "procurement_confirm",
    summaryFields: [
      { label: "任务类型", value: "采购二次确认" },
      { label: "版本", value: task.subjectVersion },
      { label: "截止", value: due.label, numeric: true },
    ],
    checkItems,
    actionLabel: actionLabels.confirmProcurement,
  }
}

function buildProcurementFamilyFixtures(): WorkItemFixture[] {
  return PROCUREMENT_CONFIRMATION_SEED.map(buildProcurementFamilyFixture)
}

/** Canonical W02 queue seed; ids align with W01 deep-links where applicable. */
export const WORK_ITEM_FIXTURES: readonly WorkItemFixture[] = [
  ...buildProcurementFamilyFixtures(),
  {
    id: "wi_card_01",
    workItemType: "CARD_FUNDS_REVIEW",
    workItemTypeLabel: "卡券票款复核",
    family: "finance",
    handlerKey: "card_funds_review",
    handlerHref: "/finance/card-funds-review?currentWorkItemId=wi_card_01",
    completionAction: "COMPLETE_CARD_FUNDS_REVIEW",
    businessObject: "销售单 · XS20260325008",
    counterparty: "蓝湾集团",
    enteredAt: "昨天 15:20",
    enteredDateTime: "2026-07-31T15:20:00+08:00",
    dueAt: "已超期 1 小时",
    dueDateTime: "2026-08-01T09:00:00+08:00",
    responsibleParty: "财务部 · 待领取",
    reason: "商城回款与开票金额待与 ERP 应收对齐",
    impact: "未复核前票款数据不可作为经营结果",
    impactSensitive: "待复核回款 " + money(42000),
    statusCode: "UNCLAIMED",
    status: { label: "待领取", tone: "info" },
    priority: 1,
    priorityLabel: "紧急",
    subjectVersion: "v1",
    allowedActions: ["CLAIM"],
    closeAllowed: false,
    scopeTags: ["待领取"],
    processorGroup: "card_funds",
    summaryFields: [
      { label: "任务类型", value: "卡券票款复核" },
      { label: "应收余额", value: money(86000) },
      { label: "待复核回款", value: money(42000) },
    ],
    actionLabel: "领取并处理",
  },
  {
    id: "wi_receipt_01",
    workItemType: "RECEIPT_FACT_REVIEW",
    workItemTypeLabel: "回款核对",
    family: "finance",
    handlerKey: "receipt_fact_review",
    handlerHref: "/finance/customer-accounts?q=海纳教育",
    completionAction: "CONFIRM_RECEIPT_FACT",
    businessObject: "销售单 · XS20260326009",
    counterparty: "海纳教育科技有限公司",
    enteredAt: "今天 09:34",
    enteredDateTime: "2026-08-01T09:34:00+08:00",
    dueAt: "今天 16:00",
    dueDateTime: "2026-08-01T16:00:00+08:00",
    responsibleParty: "财务部 · 陈琳",
    reason: "银行流水与客户结算主体名称存在简称差异",
    impact: "复核前不计入已回款金额",
    impactSensitive: "流水金额 " + money(31800),
    statusCode: "PENDING",
    status: { label: "待复核", tone: "warning" },
    priority: 3,
    priorityLabel: "普通",
    subjectVersion: "v1",
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "COMPLETE"],
    closeAllowed: false,
    scopeTags: ["团队"],
    processorGroup: "receipt_review",
    summaryFields: [
      { label: "任务类型", value: "回款核对" },
      { label: "版本", value: "v1" },
      { label: "截止", value: "今天 16:00", numeric: true },
    ],
    actionLabel: "复核通过",
  },
  {
    id: "wi_map_01",
    workItemType: "MAPPING_EXCEPTION",
    workItemTypeLabel: "映射异常处理",
    family: "exception",
    handlerKey: "mall_sync_exception",
    handlerHref: "/governance/mall-sync?view=mapping&workItemId=wi_map_01&mappingTaskId=mt_cat_01&demoRole=operations",
    completionAction: "RESOLVE_MAPPING_EXCEPTION",
    businessObject: "同步批次 · SYNC-20260801-017",
    counterparty: "华东商城",
    enteredAt: "今天 09:10",
    enteredDateTime: "2026-08-01T09:10:00+08:00",
    dueAt: "今天 18:00",
    dueDateTime: "2026-08-01T18:00:00+08:00",
    responsibleParty: "运营 · 李倩",
    reason: "供应商商品缺少公司商品池映射",
    impact: "阻断 12 条消费订单入账",
    statusCode: "IN_PROGRESS",
    status: { label: "处理中", tone: "info" },
    priority: 2,
    priorityLabel: "高",
    subjectVersion: "v3",
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "QUERY_RESULT", "COMPLETE", "TRANSFER"],
    closeAllowed: false,
    scopeTags: ["团队", "我的待办"],
    processorGroup: "mapping_exception",
    summaryFields: [
      { label: "任务类型", value: "映射异常" },
      { label: "影响对象", value: "12 条消费订单" },
      { label: "责任角色", value: "运营映射" },
      { label: "截止", value: "今天 18:00", numeric: true },
    ],
    actionLabel: actionLabels.handleMappingException,
  },
  {
    id: "wi_sync_01",
    workItemType: "MALL_SYNC_FAILURE",
    workItemTypeLabel: "商城同步异常",
    family: "exception",
    handlerKey: "mall_sync_exception",
    handlerHref: "/governance/mall-sync?view=jobs&jobId=job_inc_017&demoRole=admin",
    completionAction: "RESOLVE_SYNC_FAILURE",
    businessObject: "同步批次 · SYNC-20260801-017",
    counterparty: "商城销售单数据",
    enteredAt: "今天 09:21",
    enteredDateTime: "2026-08-01T09:21:00+08:00",
    dueAt: "今天 10:21",
    dueDateTime: "2026-08-01T10:21:00+08:00",
    responsibleParty: "系统管理员",
    reason: "2 张卡券销售单的客户映射未命中",
    impact: "ERP 数据更新延迟，商城原单不受影响",
    statusCode: "PENDING",
    status: { label: "同步异常", tone: "destructive" },
    priority: 2,
    priorityLabel: "高",
    subjectVersion: "v1",
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "QUERY_RESULT", "COMPLETE"],
    closeAllowed: false,
    scopeTags: ["我的待办", "团队"],
    processorGroup: "mapping_exception",
    summaryFields: [
      { label: "任务类型", value: "商城同步异常" },
      { label: "版本", value: "v1" },
      { label: "截止", value: "今天 10:21", numeric: true },
    ],
    actionLabel: "去处理同步异常",
  },
  {
    id: "wi_ful_01",
    workItemType: "FULFILLMENT_EXCEPTION",
    workItemTypeLabel: "履约异常",
    family: "fulfillment",
    handlerKey: "fulfillment_exception",
    handlerHref:
      "/fulfillment?lane=procurement&scope=mine&currentWorkItemId=wi_ff_electronic_01",
    completionAction: "RESOLVE_FULFILLMENT_EXCEPTION",
    businessObject: "销售单 · XS20260312008",
    counterparty: "云帆物流集团",
    enteredAt: "昨天 14:06",
    enteredDateTime: "2026-07-31T14:06:00+08:00",
    dueAt: "已超期 1 小时",
    dueDateTime: "2026-08-01T08:30:00+08:00",
    responsibleParty: "履约组 · 刘青",
    reason: "电子交付批次尚未回传接收结果",
    impact: "200 份夜班餐补包状态待确认",
    statusCode: "PENDING",
    status: { label: "已超期", tone: "destructive" },
    priority: 1,
    priorityLabel: "紧急",
    subjectVersion: "v1",
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "COMPLETE", "TRANSFER"],
    closeAllowed: false,
    scopeTags: ["团队"],
    processorGroup: "fulfillment_exception",
    summaryFields: [
      { label: "任务类型", value: "履约异常" },
      { label: "版本", value: "v1" },
      { label: "截止", value: "已超期", numeric: true },
    ],
    actionLabel: "记录处理结论",
  },
  {
    id: "wi_contract_01",
    workItemType: "CONTRACT_INFO_SUPPLEMENT",
    workItemTypeLabel: "合同信息补全",
    family: "approval",
    handlerKey: "contract_supplement",
    handlerHref: "/sales/contracts?q=HT-2026-0312",
    completionAction: "COMPLETE_CONTRACT_SUPPLEMENT",
    businessObject: "合同 · HT-2026-0312",
    counterparty: "星河制造股份有限公司",
    enteredAt: "今天 09:05",
    enteredDateTime: "2026-08-01T09:05:00+08:00",
    dueAt: "今天 17:00",
    dueDateTime: "2026-08-01T17:00:00+08:00",
    responsibleParty: "销售部 · 王敏",
    reason: "付款条件缺少客户确认附件",
    impact: "不影响看单，提交变更前必须补齐",
    statusCode: "PENDING",
    status: { label: "待补充", tone: "info" },
    priority: 3,
    priorityLabel: "普通",
    subjectVersion: "v1",
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "COMPLETE"],
    closeAllowed: false,
    scopeTags: ["我的待办"],
    processorGroup: "contract_supplement",
    summaryFields: [
      { label: "任务类型", value: "合同信息补全" },
      { label: "版本", value: "v1" },
      { label: "截止", value: "今天 17:00", numeric: true },
    ],
    actionLabel: "确认已补全",
  },
  {
    id: "wi_margin_01",
    workItemType: "LOW_MARGIN_MANAGER_CONFIRMATION",
    workItemTypeLabel: "低毛利经理确认",
    family: "approval",
    handlerKey: "low_margin_confirm",
    handlerHref: "/sales/orders/so_1003",
    completionAction: "CONFIRM_LOW_MARGIN",
    businessObject: "销售单 · XS20260328005",
    counterparty: "星河制造股份有限公司",
    enteredAt: "今天 09:05",
    enteredDateTime: "2026-08-01T09:05:00+08:00",
    dueAt: "今天 17:00",
    dueDateTime: "2026-08-01T17:00:00+08:00",
    responsibleParty: "销售领导 · 王敏",
    reason: "毛利低于组织阈值，需经理确认后继续",
    impact: "未确认前销售单不可进入履约",
    statusCode: "PENDING",
    status: { label: "待处理", tone: "warning" },
    priority: 2,
    priorityLabel: "高",
    subjectVersion: "v1",
    allowedActions: ["DEFER", "SAVE_EVIDENCE", "COMPLETE"],
    closeAllowed: false,
    scopeTags: ["我的待办"],
    processorGroup: "low_margin",
    summaryFields: [
      { label: "任务类型", value: "低毛利经理确认" },
      { label: "版本", value: "v1" },
      { label: "截止", value: "今天 17:00", numeric: true },
    ],
    actionLabel: "确认",
  },
  {
    id: "wi_dup_01",
    workItemType: "DUPLICATE_TASK_CLEANUP",
    workItemTypeLabel: "重复任务清理",
    family: "exception",
    handlerKey: "duplicate_cleanup",
    completionAction: "CLOSE_DUPLICATE",
    businessObject: "销售单 · XS20260328001",
    counterparty: "星河制造股份有限公司",
    enteredAt: "今天 09:50",
    enteredDateTime: "2026-08-01T09:50:00+08:00",
    dueAt: "今天 12:00",
    dueDateTime: "2026-08-01T12:00:00+08:00",
    responsibleParty: "系统 · 待处理",
    reason: "与已有任务指向同一确认事项，系统判定为重复",
    impact: "关闭后不影响业务记录，仅清理待办噪声",
    statusCode: "PENDING",
    status: { label: "待处理", tone: "neutral" },
    priority: 3,
    priorityLabel: "普通",
    subjectVersion: "v1",
    allowedActions: ["CLOSE", "DEFER"],
    closeAllowed: true,
    scopeTags: ["我的待办"],
    processorGroup: "duplicate_cleanup",
    summaryFields: [
      { label: "任务类型", value: "重复任务清理" },
      { label: "保留任务", value: "采购确认任务 · 销售单 XS20260328001" },
      { label: "处理方式", value: "关闭重复任务，保留已有任务" },
    ],
    actionLabel: "关闭重复任务",
  },
]

export const FAMILY_LABELS: Record<WorkItemFamily, string> = {
  approval: "审批",
  finance: "财务",
  fulfillment: "履约",
  exception: "异常",
  procurement: "采购确认",
}

/** Map W01 dashboard task ids → W02 work item deep-link targets. */
export const W01_TO_W02_WORK_ITEM: Readonly<
  Record<string, { workItemId: string; family: WorkItemFamily }>
> = {
  task_confirm_01: { workItemId: "wi_pc_01", family: "procurement" },
  task_confirm_02: { workItemId: "wi_pc_02", family: "procurement" },
  task_contract_01: { workItemId: "wi_contract_01", family: "approval" },
  task_sync_01: { workItemId: "wi_sync_01", family: "exception" },
  task_receipt_01: { workItemId: "wi_receipt_01", family: "finance" },
  task_fulfillment_01: { workItemId: "wi_ful_01", family: "fulfillment" },
  wi_pc_01: { workItemId: "wi_pc_01", family: "procurement" },
  wi_pc_02: { workItemId: "wi_pc_02", family: "procurement" },
  wi_card_01: { workItemId: "wi_card_01", family: "finance" },
  wi_sync_01: { workItemId: "wi_sync_01", family: "exception" },
  wi_receipt_01: { workItemId: "wi_receipt_01", family: "finance" },
  wi_ful_01: { workItemId: "wi_ful_01", family: "fulfillment" },
  wi_map_01: { workItemId: "wi_map_01", family: "exception" },
}

export function buildW02TaskHref(options: {
  workItemId: string
  family?: WorkItemFamily
  scope?: "mine" | "role_pool" | "team"
}): string {
  const params = new URLSearchParams()
  params.set("scope", options.scope ?? "mine")
  if (options.family) params.set("family", options.family)
  params.set("currentWorkItemId", options.workItemId)
  params.set("queueContextId", `queue:W02:${options.scope ?? "mine"}`)
  return `/workspace/tasks?${params.toString()}`
}
