"use client"

import { BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    displayActorName,
    displayReadableName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { WorkspaceWorkItem } from "../types"

/** 在原审批详情内展示本次驳回原因；完整记录沿用审批详情的历史入口。 */
export function WorkspaceRejectionNotice({
    item,
    actionable,
}: {
    item: WorkspaceWorkItem
    actionable: boolean
}) {
    const instanceId = item.approvalProcessInstanceId ?? ""
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId, order: "desc" },
        Boolean(instanceId),
    )
    const records = history.data?.pages.flatMap((page) => page.items) ?? []
    const rejection =
        records.find((record) => record.result === "REJECTED") ??
        history.data?.pages[0]?.route?.nodes.find(
            (node) => node.execution?.result === "REJECTED",
        )?.execution
    const rejectedAt = displayUnixSeconds(rejection?.decidedAt)
    const rejector = displayActorName(
        rejection?.assigneeName,
        rejection?.decidedBy,
    )
    const node = displayReadableName(rejection?.nodeName, rejection?.nodeKey)
    const reason =
        rejection?.decisionReason?.trim() ||
        item.approval?.lastRejectReason?.trim()
    return (
        <div className="space-y-3 py-5">
            <Alert variant="destructive">
                <AlertTitle>本次审批已驳回</AlertTitle>
                <AlertDescription className="space-y-2">
                    {(rejector || node || rejectedAt) && (
                        <p className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                            {rejector && <span>驳回人：{rejector}</span>}
                            {node && <span>审批节点：{node}</span>}
                            {rejectedAt && (
                                <time dateTime={rejectedAt.dateTime}>
                                    {rejectedAt.label}
                                </time>
                            )}
                        </p>
                    )}
                    {reason ? (
                        <p className="whitespace-pre-wrap break-words">
                            {reason}
                        </p>
                    ) : (
                        <p>
                            {instanceId && history.isPending
                                ? "正在读取驳回意见…"
                                : "未读取到驳回意见，请核对本次审批记录。"}
                        </p>
                    )}
                </AlertDescription>
            </Alert>
            {actionable && (
                <p className="text-xs text-muted-foreground">
                    请按驳回意见修改或补充说明后重新提交。保存修改不结束待办；重新提交将从第一个节点审批。
                </p>
            )}
            {history.isError && (
                <BusinessFailureState
                    id="workspace-rejection-history-retry"
                    title="驳回意见读取失败"
                    error={history.error}
                    onRetry={() => void history.refetch()}
                />
            )}
        </div>
    )
}
