"use client"

import { StatusBadge } from "@/components/ui/status-badge"
import { useFulfillmentGate } from "../hooks/use-fulfillment-gate"
import { usePurchasePayment } from "../hooks/use-purchase-payment"
import { purchasePaymentStatus } from "../lib/purchase-payment"
import { isBlockedWorkItem } from "../lib/work-item"
import {
    workspaceFulfillmentDescriptor,
    type WorkspaceFulfillmentDescriptor,
} from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"

/** 列表和详情使用同一付款事实来源，不以付款申请审批代替实付。 */
export function WorkspaceFulfillmentBadges({
    item,
}: {
    item: WorkspaceWorkItem
}) {
    const descriptor = workspaceFulfillmentDescriptor(item)
    if (!descriptor || descriptor.operationTypes[0] === "RECEIPT") return null

    return (
        <span className="flex flex-wrap items-center gap-1.5">
            {descriptor.role === "procurement" ? (
                <PurchasePaymentBadge workItemId={item.workItemId} />
            ) : null}
            <FulfillmentGateBadge item={item} descriptor={descriptor} />
        </span>
    )
}

function PurchasePaymentBadge({ workItemId }: { workItemId: string }) {
    const query = usePurchasePayment(workItemId)
    return query.isError || !query.data ? (
        <StatusBadge
            label={query.isError ? "付款状态待核对" : "付款状态查询中"}
            tone="neutral"
        />
    ) : (
        <StatusBadge {...purchasePaymentStatus(query.data)} />
    )
}

/** 只有服务端明确放行且无提交阻断时才显示可发货；读取失败不沿用旧结果。 */
function FulfillmentGateBadge({
    item,
    descriptor,
}: {
    item: WorkspaceWorkItem
    descriptor: WorkspaceFulfillmentDescriptor
}) {
    const query = useFulfillmentGate(
        item.workItemId,
        item.businessObjectId,
        descriptor.operationTypes[0],
    )
    const gate = query.data
    const action = ["SUPPLIER_DIRECT", "WAREHOUSE_SHIP"].includes(
        descriptor.operationTypes[0],
    )
        ? "发货"
        : "履约"
    if (query.isError || !gate) {
        return (
            <StatusBadge
                label={`${action}条件${query.isPending ? "查询中" : "待核对"}`}
                tone="neutral"
            />
        )
    }
    const blocker = item.actionBlockers.find(
        (entry) => entry.action === "PROCESS" || entry.action === "POST",
    )
    const blocked = isBlockedWorkItem(item) || Boolean(blocker)
    const allowed =
        !blocked && (gate === "SATISFIED" || gate === "NOT_APPLICABLE")
    return (
        <StatusBadge
            label={allowed ? `可${action}` : `暂不可${action}`}
            tone={allowed ? "success" : "warning"}
            title={
                blocker?.message ||
                (blocked
                    ? "当前任务受阻，请打开事项核对原因"
                    : gate === "BLOCKED"
                      ? "先款条件未满足，请完成付款后刷新核对"
                      : gate === "NOT_APPLICABLE"
                        ? "按约定无需预付，提交时将再次核对条件"
                        : "付款条件已满足，提交时将再次核对条件")
            }
        />
    )
}
