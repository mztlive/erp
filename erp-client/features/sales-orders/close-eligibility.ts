import type {
  CloseEligibility,
  ProgressTrack,
  SalesOrderNature,
} from "@/features/sales-orders/types"

/**
 * 关闭规则（W05 §5.3 / §12）：
 * - 非卡券：履约以验收完成判定
 * - 卡券：履约以履约期限到期完成，不因已消费完提前完成
 * - 关闭门槛：履约完成 AND 应收结清；开票未完成不阻塞
 */
export function computeCloseEligibility(input: {
  nature: SalesOrderNature
  fulfillment: ProgressTrack
  collection: ProgressTrack
  invoicing: ProgressTrack
  amountGross: string
  receivedAmount: string
  primaryStatusLabel: string
}): CloseEligibility {
  const {
    nature,
    fulfillment,
    collection,
    invoicing,
    amountGross,
    receivedAmount,
    primaryStatusLabel,
  } = input

  if (
    primaryStatusLabel === "已关闭" ||
    primaryStatusLabel === "已作废" ||
    primaryStatusLabel === "草稿"
  ) {
    const closed = primaryStatusLabel === "已关闭"
    return {
      fulfillmentComplete: closed || fulfillment.label === "已完成",
      receivableSettled: closed || collection.label === "已结清",
      invoiceComplete: invoicing.label === "已完成",
      eligibleToClose: closed,
      blockers: closed
        ? []
        : primaryStatusLabel === "已作废"
          ? ["销售单已作废，不适用关闭"]
          : ["草稿未生效，不适用关闭"],
      note: closed
        ? "履约完成且应收已结清，系统已自动关闭。开票状态不影响关闭。"
        : primaryStatusLabel === "已作废"
          ? "作废单保留历史提交与驳回记录，不可关闭也不可恢复。"
          : "草稿尚未进入正式状态。",
    }
  }

  const fulfillmentComplete =
    fulfillment.label === "已完成" ||
    (nature === "card_voucher" && fulfillment.label === "期限已到期")

  const receivableSettled =
    collection.label === "已结清" ||
    parseAmount(receivedAmount) >= parseAmount(amountGross) - 0.005

  const invoiceComplete = invoicing.label === "已完成"
  const blockers: string[] = []

  if (!fulfillmentComplete) {
    blockers.push(
      nature === "card_voucher"
        ? "卡券履约尚未到期完成（不因已消费完提前完成）"
        : "非卡券履约尚未验收完成"
    )
  }
  if (!receivableSettled) {
    blockers.push("应收尚未结清")
  }

  const eligibleToClose = fulfillmentComplete && receivableSettled

  return {
    fulfillmentComplete,
    receivableSettled,
    invoiceComplete,
    eligibleToClose,
    blockers,
    note: eligibleToClose
      ? "关闭条件已满足：履约完成且应收结清。系统将自动关闭；开票未完成不阻塞关闭，页面无人工关闭按钮。"
      : `关闭条件未满足：${blockers.join("；")}。开票进度不参与关闭门槛。`,
  }
}

function parseAmount(value: string): number {
  const n = Number.parseFloat(value.replace(/,/g, ""))
  return Number.isFinite(n) ? n : 0
}

/** 正式单不可直接编辑；ERP 主责正式单可发起销售变更。 */
export function canStartSalesChange(input: {
  ownerSystem: "erp" | "mall"
  primaryStatusLabel: string
  hasActiveChangeOrder: boolean
}): { allowed: boolean; reason?: string } {
  if (input.ownerSystem !== "erp") {
    return {
      allowed: false,
      reason: "当前由商城主责，ERP 不可发起销售变更；一期商业字段只读。",
    }
  }
  if (
    input.primaryStatusLabel === "草稿" ||
    input.primaryStatusLabel === "已作废" ||
    input.primaryStatusLabel === "已关闭"
  ) {
    return {
      allowed: false,
      reason: `状态「${input.primaryStatusLabel}」不可发起销售变更。`,
    }
  }
  if (
    input.primaryStatusLabel === "待销售处理" ||
    input.primaryStatusLabel === "待二次确认" ||
    input.primaryStatusLabel === "待销售领导审批" ||
    input.primaryStatusLabel === "待运营审批"
  ) {
    return {
      allowed: false,
      reason: "生效前处理中，请先完成确认/审批或驳回出路，不可并行发起变更。",
    }
  }
  if (input.hasActiveChangeOrder) {
    return {
      allowed: false,
      reason: "同一基准版本已有进行中的销售变更单。",
    }
  }
  return { allowed: true }
}
