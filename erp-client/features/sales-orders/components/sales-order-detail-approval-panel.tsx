"use client"

import { detailApprovalSummaryClassName } from "@/components/business/detail-presentation"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"

/** 销售单详情只读展示审批，决定统一在工作台办理。 */
export function ApprovalPanel({ order }: { order: SalesOrderDetailView }) {
    return (
        <ApprovalReadonly
            id="sales-order-approval"
            approval={order.approval}
            className={detailApprovalSummaryClassName}
        />
    )
}
