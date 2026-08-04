import type {
  CloseEligibility,
  ProgressTrack,
  SalesOrderNature,
} from "@/features/sales-orders/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/**
 * 结案规则（W05 §5.3 / §12）：
 * - 非卡券：交付以客户验收完成判定
 * - 卡券：交付以履约期限到期完成，不因已消费完提前完成
 * - 结案门槛：交付完成 AND 回款收齐；开票未完成不挡结案
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
          ? ["本单已作废，不会再结案"]
          : ["草稿尚未生效，谈不上结案"],
      note: closed
        ? "交付与回款都已完成，本单已自动结案。开票是否做完不影响结案。"
        : primaryStatusLabel === "已作废"
          ? "作废单只保留历史记录，不能结案，也不能恢复。"
          : "草稿还没生效，先完成提交与确认。",
    }
  }

  const fulfillmentComplete =
    fulfillment.label === "已完成" ||
    (nature === "card_voucher" && fulfillment.label === "期限已到期")

  const receivableSettled =
    collection.label === "已结清" ||
    amountAtLeast(receivedAmount, amountGross)

  const invoiceComplete = invoicing.label === "已完成"
  const blockers: string[] = []

  if (!fulfillmentComplete) {
    blockers.push(
      nature === "card_voucher"
        ? "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
        : "客户验收还没做完"
    )
  }
  if (!receivableSettled) {
    blockers.push("客户回款还没收齐")
  }

  const eligibleToClose = fulfillmentComplete && receivableSettled

  return {
    fulfillmentComplete,
    receivableSettled,
    invoiceComplete,
    eligibleToClose,
    blockers,
    note: eligibleToClose
      ? "交付和回款都齐了，系统会自动结案。发票开没开完都不挡结案，也无需人工点「关闭」。"
      : `还不能结案：${blockers.join("；")}。开票进度不参与是否结案。`,
  }
}

function amountAtLeast(receivedAmount: string, amountGross: string): boolean {
  try {
    return (
      compareDecimal(
        receivedAmount.replaceAll(",", ""),
        amountGross.replaceAll(",", ""),
        2
      ) >= 0
    )
  } catch {
    return false
  }
}

/** 已生效单不可直接编辑；ERP 主责已生效单可发起销售变更。 */
export function canStartSalesChange(input: {
  ownerSystem: "erp" | "mall"
  primaryStatusLabel: string
  hasActiveChangeOrder: boolean
}): { allowed: boolean; reason?: string } {
  if (input.ownerSystem !== "erp") {
    return {
      allowed: false,
      reason: "这单目前由商城侧维护，请在商城改内容；本系统只能查看。",
    }
  }
  if (
    input.primaryStatusLabel === "草稿" ||
    input.primaryStatusLabel === "已作废" ||
    input.primaryStatusLabel === "已关闭"
  ) {
    return {
      allowed: false,
      reason: `当前状态是「${input.primaryStatusLabel}」，不能发起改单。`,
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
      reason: "本单还在确认/审批中，请先处理完当前待办，再发起改单。",
    }
  }
  if (input.hasActiveChangeOrder) {
    return {
      allowed: false,
      reason: "已有一笔改单在处理中，请等它走完再发起新的。",
    }
  }
  return { allowed: true }
}
