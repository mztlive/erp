import type { StatusTone } from "@/components/ui/status-badge"
import type { PurchasePaymentArrangement } from "../api/payment-arrangement"

export type PurchasePaymentRequestStatus =
    PurchasePaymentArrangement["requests"][number]["status"]

/** 采购详情与工作台沿用同一套申请状态文案与色调。 */
export const purchasePaymentRequestStatusLabel: Record<
    PurchasePaymentRequestStatus,
    string
> = {
    draft: "草稿待提交",
    in_approval: "财务审核中",
    approved: "已批准待付款",
    partially_paid: "部分付款",
    paid: "已付清",
    rejected: "已驳回",
    withdrawn: "已撤回",
    closed: "已关闭",
}

export const purchasePaymentRequestStatusTone: Record<
    PurchasePaymentRequestStatus,
    StatusTone
> = {
    draft: "neutral",
    in_approval: "info",
    approved: "warning",
    partially_paid: "warning",
    paid: "success",
    rejected: "destructive",
    withdrawn: "neutral",
    closed: "neutral",
}
