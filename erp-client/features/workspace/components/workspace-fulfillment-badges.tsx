"use client"

import { BusinessStatusBadge } from "@/components/business/values"
import type { StatusTone } from "@/components/ui/status-badge"
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

export function fulfillmentPaymentStatus(item: WorkspaceWorkItem) {
    const state = item.fulfillmentStatus?.payment_state
    return state ? PAYMENT_STATUS[state] : undefined
}

/** 仅消费服务端列表摘要，不为每行发起付款或履约详情请求。 */
export function WorkspaceFulfillmentBadges({
    item,
}: {
    item: WorkspaceWorkItem
}) {
    const descriptor = workspaceFulfillmentDescriptor(item)
    if (!descriptor || descriptor.operationTypes[0] === "RECEIPT") return null
    const payment = fulfillmentPaymentStatus(item)

    return (
        <span className="flex flex-wrap items-center gap-1.5">
            {descriptor.role === "procurement" ? (
                <BusinessStatusBadge
                    appearance={payment?.tone === "success" ? "quiet" : "badge"}
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

/** 按任务状态与提交限制显示处理资格；付款状态仅作提醒。 */
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
    const blocker = item.actionBlockers.find(
        (entry) => entry.action === "PROCESS" || entry.action === "POST",
    )
    const blocked = isBlockedWorkItem(item) || Boolean(blocker)
    const allowed = !blocked
    return (
        <BusinessStatusBadge
            label={allowed ? `可${action}` : `暂不可${action}`}
            tone={allowed ? "success" : "warning"}
            appearance={allowed ? "quiet" : "badge"}
            title={
                blocker?.message ||
                (blocked
                    ? "当前任务受阻，请打开事项核对原因"
                    : gate === "BLOCKED"
                      ? "预付待完成，不影响确认实际履约"
                      : gate === "NOT_APPLICABLE"
                        ? "当前无预付要求，提交时核对来源与数量"
                        : "提交时核对来源与数量")
            }
        />
    )
}
