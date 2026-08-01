import { computeCloseEligibility } from "@/features/sales-orders/close-eligibility"
import type {
  ActionBlocker,
  CardSalesApproval,
  FormalAllowedAction,
  ProcurementRejectionResolution,
  SalesOrderListItem,
  SalesOrderNature,
  SalesOrderOwner,
  SalesOrderRelatedSummary,
  SalesOrderRevisionSnapshot,
  ProgressTrack,
  SalesOrderLineItem,
  SalesChangeOrderSummary,
} from "@/features/sales-orders/types"

type BuildInput = {
  id: string
  documentNumber: string
  customerName: string
  contractNumber: string
  contractRevisionLabel?: string
  nature: SalesOrderNature
  originSystem: SalesOrderOwner
  ownerSystem: SalesOrderOwner
  primaryStatus: { label: string; tone: SalesOrderListItem["primaryStatus"]["tone"] }
  fulfillment: ProgressTrack
  collection: ProgressTrack
  invoicing: ProgressTrack
  amountGross: string
  amountNet: string
  taxAmount: string
  receivedAmount: string
  invoicedAmount: string
  ownerName: string
  submittedAt: string
  welfareScene: string
  remark?: string
  version: number
  lockVersion?: number
  settlementEntity: string
  sellerEntity: string
  paymentTerms: string
  fulfillmentDeadline: string
  customerContact?: string
  lineItems: readonly SalesOrderLineItem[]
  related: SalesOrderRelatedSummary
  revisions?: readonly SalesOrderRevisionSnapshot[]
  procurementRejection?: ProcurementRejectionResolution | null
  activeCardSalesApproval?: CardSalesApproval | null
  activeChangeOrder?: SalesChangeOrderSummary | null
}

/**
 * 统一装配列表/对象记录：创建来源与主责分列、关闭资格、只读边界与允许动作。
 */
export function buildSalesOrder(input: BuildInput): SalesOrderListItem {
  const commercialReadOnly =
    input.ownerSystem === "mall" ||
    input.primaryStatus.label === "已关闭" ||
    input.primaryStatus.label === "已作废" ||
    Boolean(input.activeCardSalesApproval) ||
    (input.procurementRejection != null &&
      input.procurementRejection.reviewStatus !== "RESOLVED" &&
      input.procurementRejection.reviewStatus !== "VOIDED" &&
      input.primaryStatus.label === "待销售处理")

  const commercialReadOnlyReason =
    input.ownerSystem === "mall"
      ? "当前由商城主责：一期卡券商业字段在 ERP 只读，二期迁移仅改主责不换单号/版本。"
      : input.activeCardSalesApproval
        ? "卡券销售审批进行中：冻结提交只读，须通过任务处理页决定。"
        : input.primaryStatus.label === "已关闭" ||
            input.primaryStatus.label === "已作废"
          ? "终态不可直接编辑；历史版本记录不被当前主数据覆盖。"
          : commercialReadOnly
            ? "商业内容只读；变更须走销售变更单。"
            : undefined

  const closeEligibility = computeCloseEligibility({
    nature: input.nature,
    fulfillment: input.fulfillment,
    collection: input.collection,
    invoicing: input.invoicing,
    amountGross: input.amountGross,
    receivedAmount: input.receivedAmount,
    primaryStatusLabel: input.primaryStatus.label,
  })

  const revisions: readonly SalesOrderRevisionSnapshot[] =
    input.revisions ??
    defaultRevisions({
      version: input.version,
      contractNumber: input.contractNumber,
      contractRevisionLabel: input.contractRevisionLabel,
      customerName: input.customerName,
      amountGross: input.amountGross,
      lineItems: input.lineItems,
      submittedAt: input.submittedAt,
    })

  const allowedActions: FormalAllowedAction[] = ["VIEW_CLOSE_CONDITIONS", "PRINT"]
  const actionBlockers: ActionBlocker[] = []

  const formal =
    input.primaryStatus.label !== "草稿" &&
    input.primaryStatus.label !== "已作废"

  if (formal) {
    allowedActions.push("EXPORT")
  }

  if (
    input.nature === "physical_service" &&
    (input.primaryStatus.label === "履约中" ||
      input.primaryStatus.label === "已生效") &&
    input.ownerSystem === "erp"
  ) {
    allowedActions.push("REGISTER_ACCEPTANCE")
  } else if (input.nature === "card_voucher") {
    actionBlockers.push({
      action: "REGISTER_ACCEPTANCE",
      reason: "卡券以履约期限到期完成履约，不登记客户验收。",
    })
  }

  if (
    input.ownerSystem === "erp" &&
    formal &&
    input.primaryStatus.label !== "已关闭" &&
    !input.activeChangeOrder &&
    !input.procurementRejection &&
    !input.activeCardSalesApproval
  ) {
    allowedActions.push("START_SALES_CHANGE")
  } else if (input.ownerSystem === "mall") {
    actionBlockers.push({
      action: "START_SALES_CHANGE",
      reason: "商城主责期间不可在 ERP 发起销售变更。",
    })
  } else if (input.activeChangeOrder) {
    actionBlockers.push({
      action: "START_SALES_CHANGE",
      reason: "已有进行中的销售变更单。",
    })
  }

  if (
    input.procurementRejection &&
    (input.procurementRejection.reviewStatus === "REJECTED" ||
      input.procurementRejection.reviewStatus === "PENDING_LOW_MARGIN_MANAGER")
  ) {
    allowedActions.push("RESOLVE_PROCUREMENT_REJECTION")
  }

  if (input.activeCardSalesApproval) {
    allowedActions.push("HANDLE_CARD_APPROVAL")
  }

  // 业务性质创建后锁定：不允许通过变更修改（仅数据标记）
  return {
    id: input.id,
    documentNumber: input.documentNumber,
    customerName: input.customerName,
    contractNumber: input.contractNumber,
    contractRevisionLabel:
      input.contractRevisionLabel ?? `${input.contractNumber}@v1`,
    nature: input.nature,
    originSystem: input.originSystem,
    ownerSystem: input.ownerSystem,
    primaryStatus: input.primaryStatus,
    fulfillment: input.fulfillment,
    collection: input.collection,
    invoicing: input.invoicing,
    amountGross: input.amountGross,
    amountNet: input.amountNet,
    taxAmount: input.taxAmount,
    receivedAmount: input.receivedAmount,
    invoicedAmount: input.invoicedAmount,
    ownerName: input.ownerName,
    submittedAt: input.submittedAt,
    welfareScene: input.welfareScene,
    remark: input.remark,
    version: input.version,
    lockVersion: input.lockVersion ?? input.version,
    settlementEntity: input.settlementEntity,
    sellerEntity: input.sellerEntity,
    paymentTerms: input.paymentTerms,
    fulfillmentDeadline: input.fulfillmentDeadline,
    customerContact: input.customerContact,
    lineItems: input.lineItems,
    related: input.related,
    closeEligibility,
    natureLocked: true,
    commercialReadOnly,
    commercialReadOnlyReason,
    revisions,
    procurementRejection: input.procurementRejection ?? null,
    activeCardSalesApproval: input.activeCardSalesApproval ?? null,
    activeChangeOrder: input.activeChangeOrder ?? null,
    allowedActions,
    actionBlockers,
  }
}

function defaultRevisions(input: {
  version: number
  contractNumber: string
  contractRevisionLabel?: string
  customerName: string
  amountGross: string
  lineItems: readonly SalesOrderLineItem[]
  submittedAt: string
}): SalesOrderRevisionSnapshot[] {
  const lineSummary = input.lineItems
    .map((line) => `${line.name}×${line.quantity}${line.unit}`)
    .join("；")
  const snapshots: SalesOrderRevisionSnapshot[] = []
  for (let n = 1; n <= input.version; n += 1) {
    snapshots.push({
      revisionNo: n,
      effectiveAt:
        n === input.version
          ? input.submittedAt
          : input.submittedAt.replace(/\d{2}:\d{2}$/, "09:00"),
      contractRevisionLabel:
        input.contractRevisionLabel ?? `${input.contractNumber}@v${n}`,
      customerSnapshot: `${input.customerName}（修订 v${n} 记录）`,
      amountGross: input.amountGross,
      lineSummary:
        n === input.version
          ? lineSummary
          : `${lineSummary}（历史口径保留，不被当前主数据覆盖）`,
      changeOrderId: n > 1 ? `SCO-${n - 1}` : undefined,
      note:
        n === 1
          ? "首个销售版本：合同与主数据以本修订精确记录为准。"
          : `销售变更生效形成 v${n}；既有履约/票款不被覆盖。`,
    })
  }
  return snapshots
}
