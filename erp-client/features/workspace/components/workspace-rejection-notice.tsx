"use client"

import { CircleAlertIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    displayActorName,
    displayReadableName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { WorkspaceWorkItem } from "../types"

/** 在单据正文顶部紧凑展示驳回原因，直接服务左侧修改与核对。 */
export function WorkspaceRejectionNotice({
    item,
    actionable,
}: {
    item: WorkspaceWorkItem
    actionable: boolean
}) {
    const isRejected =
        item.approval?.status === "REJECTED" ||
        item.workItemType === "APPROVAL_REWORK"
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId ?? ""
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId, order: "desc" },
        isRejected && Boolean(instanceId),
    )
    if (!isRejected) return null

    const records = history.data?.pages.flatMap((page) => page.items) ?? []
    const rejection =
        records.find((record) => record.result === "REJECTED") ??
        history.data?.pages[0]?.route?.nodes.find(
            (node) => node.execution?.result === "REJECTED",
        )?.execution
    const rejectedAt = displayUnixSeconds(rejection?.decidedAt)
    const rejector =
        displayActorName(rejection?.assigneeName, rejection?.decidedBy) ||
        displayActorName(item.approval?.lastRejectorLabel)
    const node = displayReadableName(rejection?.nodeName, rejection?.nodeKey)
    const reason =
        rejection?.decisionReason?.trim() ||
        item.approval?.lastRejectReason?.trim()

    return (
        <section
            aria-label="驳回意见"
            data-slot="workspace-rejection-notice"
            className="flex items-start gap-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/50 px-3.5 py-2.5 text-xs"
        >
            <CircleAlertIcon
                aria-hidden="true"
                className="mt-0.5 size-3.5 shrink-0 text-destructive"
            />
            <div className="min-w-0 space-y-1">
                <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5">
                    <p className="font-semibold text-destructive-soft-foreground">
                        本次审批已驳回
                    </p>
                    {rejector && (
                        <span className="font-medium text-destructive-soft-foreground/85">
                            {rejector}
                        </span>
                    )}
                    {node && (
                        <span className="text-muted-foreground">{node}</span>
                    )}
                    {rejectedAt && (
                        <time
                            dateTime={rejectedAt.dateTime}
                            className="num text-muted-foreground"
                        >
                            {rejectedAt.label}
                        </time>
                    )}
                </div>
                {reason ? (
                    <p className="text-body-compact font-medium whitespace-pre-wrap wrap-anywhere">
                        {reason}
                    </p>
                ) : (
                    <p className="text-muted-foreground">
                        {instanceId && history.isPending
                            ? "正在读取驳回意见…"
                            : "驳回意见暂不可用，请核对审批记录。"}
                    </p>
                )}
                {actionable && (
                    <p className="text-muted-foreground">
                        请按意见修改或补充说明。重提后从第一个节点审批，仅保存修改会保留当前待办。
                    </p>
                )}
                {history.isError && (
                    <div className="flex flex-wrap items-center gap-x-2 text-muted-foreground">
                        <span role="status">驳回意见读取失败。</span>
                        <Button
                            id={`workspace-rejection-${toAutomationIdSegment(item.workItemId)}-history-retry`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            disabled={history.isFetching}
                            onClick={() => void history.refetch()}
                        >
                            重试读取
                        </Button>
                    </div>
                )}
            </div>
        </section>
    )
}
