import type { RequestStatus } from "../types"
import type { StatusTone } from "@/components/ui/status-badge"

export const requestStatusLabel: Record<RequestStatus, string> = {
    draft: "草稿",
    in_approval: "待审核",
    approved: "待付款",
    partially_paid: "部分付款",
    paid: "已付清",
    rejected: "已驳回",
    withdrawn: "已撤回",
    closed: "已关闭",
}

export const requestStatusTone: Record<RequestStatus, StatusTone> = {
    draft: "neutral",
    in_approval: "info",
    approved: "warning",
    partially_paid: "warning",
    paid: "success",
    rejected: "destructive",
    withdrawn: "neutral",
    closed: "neutral",
}
