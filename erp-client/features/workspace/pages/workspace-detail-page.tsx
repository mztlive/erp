"use client"

import { useState } from "react"
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
    const router = useRouter()
    const params = useSearchParams()
    const queryClient = useQueryClient()
    const profile = useAccountProfileQuery()
    const returnTo = workspaceQueueHref(
        params.get("returnTo"),
        approval ? "/workspace?view=started" : "/workspace",
    )
    const [autoNext, setAutoNext] = useState(params.get("autoNext") === "1")
    const taskQuery = useWorkspaceTaskDetailQuery(
        approval ? undefined : id,
        profile.data,
    )
    const approvalQuery = useWorkspaceApprovalDetailQuery(
        approval ? id : undefined,
        profile.data,
    )
    const detail = approval ? approvalQuery : taskQuery
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
    const complete = (completedId: string, preferredId?: string) => {
        const items = queue.data?.items ?? []
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
        void queryClient.invalidateQueries({
            queryKey: workspaceHomeKeys.all,
            refetchType: "none",
        })
        if (autoNext && next && !queue.isError)
            router.replace(
                buildWorkspaceDetailHref("tasks", next.workItemId, {
                    returnTo,
                    autoNext: true,
                }),
            )
        else back()
    }
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
                    description="当前任务可继续办理，完成后将返回队列。"
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
