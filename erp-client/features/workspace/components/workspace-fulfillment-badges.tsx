"use client"

import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import type { WorkItemFulfillmentStatus } from "@/features/work-items/types"
import { isBlockedWorkItem } from "../lib/work-item"
import {
    workspaceFulfillmentDescriptor,
    type WorkspaceFulfillmentDescriptor,
} from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"

const PAYMENT_STATUS: Record<
    NonNullable<WorkItemFulfillmentStatus["payment_state"]>,
    { label: string; tone: StatusTone }
> = {
    UNPAID: { label: "未付款", tone: "warning" },
    PARTIAL: { label: "部分付款", tone: "info" },
    PAID: { label: "已付清", tone: "success" },
    NOT_REQUIRED: { label: "无需付款", tone: "neutral" },
}

/** 仅消费服务端列表摘要，不为每行发起付款或履约详情请求。 */
export function WorkspaceFulfillmentBadges({
    item,
}: {
    item: WorkspaceWorkItem
}) {
    const descriptor = workspaceFulfillmentDescriptor(item)
    if (!descriptor || descriptor.operationTypes[0] === "RECEIPT") return null
    const paymentState = item.fulfillmentStatus?.payment_state
    const payment = paymentState ? PAYMENT_STATUS[paymentState] : undefined

    return (
        <span className="flex flex-wrap items-center gap-1.5">
            {descriptor.role === "procurement" ? (
                <StatusBadge
                    {...(payment ?? {
                        label: "付款状态待核对",
                        tone: "neutral",
                    })}
                />
            ) : null}
            <FulfillmentGateBadge item={item} descriptor={descriptor} />
        </span>
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
    const gate = item.fulfillmentStatus?.gate_state
    const action = ["SUPPLIER_DIRECT", "WAREHOUSE_SHIP"].includes(
        descriptor.operationTypes[0],
    )
        ? "发货"
        : "履约"
    if (!gate) {
        return <StatusBadge label={`${action}条件待核对`} tone="neutral" />
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
