"use client"

import { useEffect, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { BusinessFailureState, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { WorkspaceHomeSkeleton } from "../components/workspace-home-skeleton"
import { WorkspaceProcessingLayout } from "../components/workspace-processing-layout"
import { WorkspaceTaskDetail } from "../components/workspace-task-detail"
import {
    useWorkspaceApprovalDetailQuery,
    useWorkspaceDashboardQuery,
    useWorkspaceTaskDetailQuery,
    workspaceHomeKeys,
} from "../hooks/queries"
import { useWorkspaceDetailDocumentTitle } from "../hooks/use-workspace-detail-title"
import {
    parseWorkspaceSearchParams,
    toTodayWorkspaceQuery,
} from "../lib/url-state"

/** 独立详情入口：详情按路由 ID 查询，队列仅供连续办理使用。 */
export function WorkspaceDetailPage({
    id,
    approval = false,
}: {
    id: string
    approval?: boolean
}) {
    return (
        <WorkspaceDetailContent
            key={`${approval ? "approval" : "task"}:${id}`}
            id={id}
            approval={approval}
        />
    )
}

function WorkspaceDetailContent({
    id,
    approval,
}: {
    id: string
    approval: boolean
}) {
    const router = useRouter()
    const params = useSearchParams()
    const queryClient = useQueryClient()
    const profile = useAccountProfileQuery()
    const returnTo = workspaceQueueHref(
        params.get("returnTo"),
        approval ? "/workspace?view=started" : "/workspace",
    )
    const [autoNext, setAutoNext] = useState(params.get("autoNext") === "1")
    const [completion, setCompletion] = useState<{
        completedId: string
        preferredId?: string
    } | null>(null)
    const taskQuery = useWorkspaceTaskDetailQuery(
        approval ? undefined : id,
        profile.data,
        !completion,
    )
    const approvalQuery = useWorkspaceApprovalDetailQuery(
        approval ? id : undefined,
        profile.data,
    )
    const detail = approval ? approvalQuery : taskQuery
    useWorkspaceDetailDocumentTitle(detail.data)
    const queueState = parseWorkspaceSearchParams(
        new URLSearchParams(returnTo.split("?")[1]),
    )
    const queue = useWorkspaceDashboardQuery(
        toTodayWorkspaceQuery(queueState, "Asia/Shanghai"),
        profile.data,
        !approval && autoNext,
    )
    const backLabel = approval ? "返回审批队列" : "返回任务队列"
    const back = () => router.replace(returnTo, { scroll: false })
    // 完成通知可能早于队列请求结束；只有成功取得队列才能判断是否还有下一项。
    useEffect(() => {
        if (!completion) return
        if (!autoNext || approval) {
            router.replace(returnTo, { scroll: false })
            return
        }
        if (!queue.isSuccess) return
        const { completedId, preferredId } = completion
        const items = queue.data.items
        const index = items.findIndex((item) => item.workItemId === completedId)
        const next =
            items.find(
                (item) =>
                    item.workItemId === preferredId &&
                    item.workItemId !== completedId,
            ) ??
            items
                .slice(index + 1)
                .find((item) => item.workItemId !== completedId) ??
            items.find((item) => item.workItemId !== completedId)
        if (next)
            router.replace(
                buildWorkspaceDetailHref("tasks", next.workItemId, {
                    returnTo,
                    autoNext: true,
                }),
            )
        else router.replace(returnTo, { scroll: false })
    }, [
        completion,
        autoNext,
        approval,
        queue.isSuccess,
        queue.data,
        router,
        returnTo,
    ])
    const complete = (completedId: string, preferredId?: string) => {
        setCompletion({ completedId, preferredId })
        void queryClient.invalidateQueries({
            queryKey: workspaceHomeKeys.all,
            refetchType: "none",
        })
    }
    if (completion)
        return (
            <PageScaffold>
                {autoNext && queue.isError ? (
                    <BusinessFailureState
                        title="当前任务已处理，后续任务读取失败"
                        description="无需重复审批或提交，请重试读取下一项，或返回任务队列。"
                        error={queue.error}
                        onRetry={() => void queue.refetch()}
                        action={
                            <Button
                                id="workspace-completed-back"
                                onClick={back}
                            >
                                {backLabel}
                            </Button>
                        }
                    />
                ) : (
                    <p role="status" className="text-sm text-muted-foreground">
                        {autoNext && !approval
                            ? "当前任务已处理，正在打开下一项…"
                            : "当前任务已处理，正在返回队列…"}
                    </p>
                )}
            </PageScaffold>
        )
    if (profile.isError || detail.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    title={approval ? "审批详情读取失败" : "任务详情读取失败"}
                    error={profile.error ?? detail.error}
                    onRetry={() =>
                        void (profile.isError
                            ? profile.refetch()
                            : detail.refetch())
                    }
                    action={
                        <Button
                            id="workspace-detail-failure-back"
                            onClick={back}
                        >
                            {backLabel}
                        </Button>
                    }
                />
            </PageScaffold>
        )
    if (!detail.data || profile.isPending)
        return <WorkspaceHomeSkeleton detail />
    return (
        <PageScaffold density="compact" className="min-h-0 gap-0">
            {autoNext && queue.isError && (
                <BusinessFailureState
                    title="后续任务读取失败"
                    description="当前任务可继续办理；完成后可重试读取下一项，或返回队列。"
                    error={queue.error}
                    onRetry={() => void queue.refetch()}
                />
            )}
            <WorkspaceProcessingLayout
                item={detail.data}
                autoNext={autoNext}
                onAutoNextChange={setAutoNext}
                onBack={back}
                backLabel={backLabel}
            >
                <WorkspaceTaskDetail
                    key={id}
                    item={detail.data}
                    grantedPermissions={profile.data?.permissions ?? []}
                    onDecisionApplied={(result, completedId) =>
                        complete(completedId, result.nextOpenTask?.workItemId)
                    }
                    onTaskCompleted={complete}
                />
            </WorkspaceProcessingLayout>
        </PageScaffold>
    )
}
