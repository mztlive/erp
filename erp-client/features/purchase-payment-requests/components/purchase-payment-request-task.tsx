"use client"

import { useState } from "react"
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
    const [completed, setCompleted] = useState(false)
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
        !completed,
    )
    // 完成后旧任务不再具备材料读取资格，等待队列切换时不展示其刷新错误。
    if (completed)
        return (
            <p role="status" className="p-7 text-sm text-muted-foreground">
                当前任务已处理，正在更新待办…
            </p>
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
                key={query.data.id}
                request={query.data}
                task={item}
                onDecisionApplied={(view) => {
                    setCompleted(true)
                    onDecisionApplied?.(view)
                }}
                onTaskCompleted={() => {
                    setCompleted(true)
                    onTaskCompleted?.()
                }}
            />
        </div>
    )
}
