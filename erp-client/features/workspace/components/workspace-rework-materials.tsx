"use client"

import { BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Spinner } from "@/components/ui/spinner"
import { ApprovalSubmittedMaterialsContent } from "@/features/approval-workflow/components/approval-submitted-materials"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import {
    displayActorName,
    displayReadableName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { WorkspaceWorkItem } from "../types"

/** 只读取本任务绑定的审批实例，不用原单的最新审批替代被驳回的提交。 */
export function WorkspaceReworkMaterials({
    item,
}: {
    item: WorkspaceWorkItem
}) {
    const instanceId = item.approvalProcessInstanceId ?? ""
    const id = `workspace-rework-${toAutomationIdSegment(item.workItemId)}`
    const materials = useApprovalMaterials(instanceId, Boolean(instanceId))
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
    const data = materials.data
    const matchesSubmission =
        data &&
        data.document_id === item.businessObjectId &&
        data.document_type ===
            (item.routeContext?.documentType ?? item.businessObjectType) &&
        String(data.subject_version) === item.subjectVersion

    return (
        <div className="space-y-6">
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
                    ) : item.impactSummary ? (
                        <p className="whitespace-pre-wrap break-words">
                            驳回摘要：{item.impactSummary}
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
            <p className="text-sm leading-6 text-muted-foreground">
                本次审批已结束。下方为本次被驳回的提交资料，修改保存不会改变这份资料；最新内容请查看当前原单。重新提交时须填写说明，并从第一个节点完整审批。
            </p>
            {!instanceId ? (
                <Alert>
                    <AlertTitle>暂无法读取本次提交资料</AlertTitle>
                    <AlertDescription>
                        此任务缺少审批关联，请刷新任务后重试；仍不可用时请联系管理员核对。可通过“查看当前原单”查看最新内容。
                    </AlertDescription>
                </Alert>
            ) : (
                <>
                    {history.isError ? (
                        <BusinessFailureState
                            id={`${id}-history-retry`}
                            title="本次审批记录读取失败"
                            error={history.error}
                            onRetry={() => void history.refetch()}
                        />
                    ) : (
                        <details className="rounded-lg border p-4">
                            <summary
                                id={`${id}-history-toggle`}
                                className="cursor-pointer text-sm font-medium"
                            >
                                本次审批记录
                            </summary>
                            <div className="mt-4">
                                {history.isPending ? (
                                    <p
                                        role="status"
                                        className="text-sm text-muted-foreground"
                                    >
                                        正在读取审批记录…
                                    </p>
                                ) : (
                                    <ExecutionHistory
                                        id={`${id}-history`}
                                        items={records}
                                        hasMore={history.hasNextPage}
                                        loadingMore={history.isFetchingNextPage}
                                        onLoadMore={() =>
                                            void history.fetchNextPage()
                                        }
                                        compact
                                        presentation="timeline"
                                    />
                                )}
                            </div>
                        </details>
                    )}
                    <section
                        aria-label="本次被驳回的提交资料"
                        className="min-w-0 border-t pt-6"
                    >
                        {materials.isPending ? (
                            <p
                                role="status"
                                className="flex min-h-40 items-center justify-center gap-2 text-sm text-muted-foreground"
                            >
                                <Spinner />
                                正在读取提交资料…
                            </p>
                        ) : materials.isError ? (
                            <BusinessFailureState
                                id={`${id}-materials-retry`}
                                title="本次提交资料读取失败"
                                error={materials.error}
                                onRetry={() => void materials.refetch()}
                            />
                        ) : !matchesSubmission ? (
                            <Alert variant="destructive">
                                <AlertTitle>
                                    提交资料与当前任务不一致
                                </AlertTitle>
                                <AlertDescription>
                                    请刷新任务后重试，仍不一致时请联系管理员核对。当前原单不能替代本次被驳回的资料。
                                </AlertDescription>
                            </Alert>
                        ) : (
                            <ApprovalSubmittedMaterialsContent
                                data={data}
                                instanceId={instanceId}
                                embedded
                            />
                        )}
                    </section>
                </>
            )}
        </div>
    )
}
