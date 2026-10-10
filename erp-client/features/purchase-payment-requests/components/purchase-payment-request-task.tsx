"use client"

import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { usePaymentRequestQuery } from "../hooks/queries"
import type { PaymentTaskContext } from "../types"
import { RequestDetail } from "./request-detail"

/** 使用当前任务读取申请及限定来源，发起人历史跟踪使用普通申请资格。 */
export function PurchasePaymentRequestTask({
    item,
    onDecisionApplied,
    onTaskCompleted,
}: {
    item: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
}) {
    const scoped =
        item.workItemType !== "APPROVAL_INSTANCE" &&
        item.allowedActions.some(
            (action) =>
                action === "PROCESS" ||
                action === "APPROVE" ||
                action === "REJECT",
        )
    const query = usePaymentRequestQuery(
        item.businessObjectId,
        scoped ? item.workItemId : undefined,
    )
    if (query.isPending)
        return (
            <p className="p-7 text-sm text-muted-foreground">
                正在读取付款申请…
            </p>
        )
    if (query.isError)
        return (
            <div className="space-y-3 p-7">
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(query.error, "付款申请读取失败")}
                </p>
                <Button
                    id="ppr-task-retry"
                    variant="outline"
                    onClick={() => void query.refetch()}
                >
                    重试
                </Button>
            </div>
        )
    return (
        <div className="px-7 py-6">
            <RequestDetail
                request={query.data}
                task={item}
                onDecisionApplied={onDecisionApplied}
                onTaskCompleted={onTaskCompleted}
            />
        </div>
    )
}
