"use client"

import { CircleAlertIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    displayActorName,
    displayReadableName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { WorkspaceWorkItem } from "../types"

/** 在单据正文顶部紧凑展示驳回原因，直接服务左侧修改与核对。 */
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
        rejection?.assigneeName ?? item.approval?.lastRejectorLabel,
        rejection?.decidedBy,
    )
    const node = displayReadableName(rejection?.nodeName, rejection?.nodeKey)
    const reason =
        rejection?.decisionReason?.trim() ||
        item.approval?.lastRejectReason?.trim()
    const metaParts = [rejector, node, rejectedAt?.label].filter(Boolean)

    return (
        <div className="space-y-2">
            <Alert variant="destructive" className="py-2.5">
                <CircleAlertIcon aria-hidden="true" />
                <AlertTitle className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 text-xs font-semibold">
                    <span>
                        {actionable ? "上轮驳回意见" : "本次审批已驳回"}
                    </span>
                    {metaParts.length > 0 ? (
                        <span className="num text-xs font-normal opacity-80">
                            {metaParts.join(" · ")}
                        </span>
                    ) : null}
                </AlertTitle>
                <AlertDescription className="space-y-1 text-xs">
                    {reason ? (
                        <p className="whitespace-pre-wrap break-words text-body-compact font-medium text-destructive-soft-foreground">
                            {reason}
                        </p>
                    ) : (
                        <p>
                            {instanceId && history.isPending
                                ? "正在读取驳回意见…"
                                : "未读取到驳回意见，请核对右侧审批记录。"}
                        </p>
                    )}
                    {actionable ? (
                        <p className="text-xs opacity-80">
                            请按驳回意见修改或补充说明后重新提交；保存修改不结束待办，重新提交将从第一个节点审批。
                        </p>
                    ) : null}
                </AlertDescription>
            </Alert>
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
