"use client"

import { workspaceQueueParams } from "@/lib/workspace-queue-routes"

import { useCallback, useEffect, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
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
    restrictWorkspaceFilters,
    toTodayWorkspaceQuery,
} from "../lib/url-state"

const AUTO_NEXT_STORAGE_KEY = "erp.workspace.autoNext"

function readStoredAutoNext(param: string | null): boolean {
    if (param === "1") return true
    if (param === "0") return false
    if (typeof window === "undefined") return false
    try {
        return window.localStorage.getItem(AUTO_NEXT_STORAGE_KEY) === "1"
    } catch {
        return false
    }
}

/** 独立详情入口：详情按路由 ID 查询，队列用于连续办理与上下项切换。 */
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
        approval ? "/workspace/approvals" : "/workspace/tasks",
    )
    const [autoNext, setAutoNextState] = useState(() =>
        readStoredAutoNext(params.get("autoNext")),
    )
    const setAutoNext = useCallback((next: boolean) => {
        setAutoNextState(next)
        try {
            window.localStorage.setItem(AUTO_NEXT_STORAGE_KEY, next ? "1" : "0")
        } catch {
            // 隐私模式下忽略本地存储写入失败
        }
    }, [])
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
    const item = detail.data?.item
    useWorkspaceDetailDocumentTitle(
        item?.workItemType === "APPROVAL_REWORK" ? undefined : item,
    )
    const reworkTask = detail.data?.reworkTask
    const [queuePath, queueSearch] = returnTo.split("?")
    const queueState = restrictWorkspaceFilters(
        parseWorkspaceSearchParams(
            workspaceQueueParams(queuePath, new URLSearchParams(queueSearch)),
        ),
        profile.data?.permissions,
    )
    const canReadQueue = Boolean(
        profile.data &&
        hasPermission(
            profile.data.permissions,
            queueState.view === "started"
                ? "approval_instance:read"
                : "work_item:list",
        ),
    )
    const queue = useWorkspaceDashboardQuery(
        toTodayWorkspaceQuery(queueState, "Asia/Shanghai"),
        profile.data,
        canReadQueue,
    )
    const backLabel = approval ? "返回审批队列" : "返回任务队列"
    const back = useCallback(
        () => router.replace(returnTo, { scroll: false }),
        [returnTo, router],
    )
    const handleRefresh = useCallback(() => {
        void Promise.all([
            detail.refetch(),
            queryClient.invalidateQueries({ queryKey: ["approval"] }),
            queryClient.invalidateQueries({
                queryKey: ["purchase-payment-requests"],
            }),
            queryClient.invalidateQueries({
                queryKey: ["workspace-invoice-review"],
            }),
        ])
    }, [detail, queryClient])

    // 队列上下项切换（复用列表缓存，无额外请求）
    const queueItems = queue.data?.items ?? []
    const currentIndex = queueItems.findIndex(
        (entry) => entry.workItemId === id,
    )
    const prevItem = currentIndex > 0 ? queueItems[currentIndex - 1] : undefined
    const nextItem =
        currentIndex >= 0 && currentIndex + 1 < queueItems.length
            ? queueItems[currentIndex + 1]
            : undefined
    const openSibling = useCallback(
        (targetId: string) => {
            router.replace(
                buildWorkspaceDetailHref(
                    approval ? "approvals" : "tasks",
                    targetId,
                    { returnTo, autoNext: !approval && autoNext },
                ),
            )
        },
        [approval, autoNext, returnTo, router],
    )
    const queueNavigation =
        currentIndex >= 0 && queueItems.length > 1 ? (
            <div className="flex items-center gap-1">
                <span className="num mr-1 text-xs text-muted-foreground">
                    第 {currentIndex + 1} / {queueItems.length} 项
                </span>
                <Button
                    id="workspace-processing-prev"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={!prevItem}
                    aria-label="上一项"
                    onClick={() => prevItem && openSibling(prevItem.workItemId)}
                >
                    <ChevronLeftIcon aria-hidden="true" />
                    上一项
                </Button>
                <Button
                    id="workspace-processing-next"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={!nextItem}
                    aria-label="下一项"
                    onClick={() => nextItem && openSibling(nextItem.workItemId)}
                >
                    下一项
                    <ChevronRightIcon aria-hidden="true" />
                </Button>
            </div>
        ) : null

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
        const index = items.findIndex(
            (entry) => entry.workItemId === completedId,
        )
        const next =
            items.find(
                (entry) =>
                    entry.workItemId === preferredId &&
                    entry.workItemId !== completedId,
            ) ??
            items
                .slice(index + 1)
                .find((entry) => entry.workItemId !== completedId) ??
            items.find((entry) => entry.workItemId !== completedId)
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
            <DetailWorkspacePage>
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
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
                            <div
                                role="status"
                                className="flex min-h-48 flex-col items-center justify-center gap-3 py-12 text-center"
                            >
                                <div className="flex items-center gap-2 text-body-compact text-muted-foreground">
                                    <Spinner />
                                    <span>
                                        {autoNext && !approval
                                            ? "当前任务已处理，正在打开下一项…"
                                            : "当前任务已处理，正在返回队列…"}
                                    </span>
                                </div>
                                <Button
                                    id="workspace-completed-back"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    onClick={back}
                                >
                                    {backLabel}
                                </Button>
                            </div>
                        )}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (profile.isError || detail.isError)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            title={
                                approval
                                    ? "审批详情读取失败"
                                    : "任务详情读取失败"
                            }
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
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    if (!detail.data || profile.isPending)
        return <WorkspaceHomeSkeleton detail />
    return (
        <DetailWorkspacePage className="min-h-0">
            {autoNext && queue.isError && (
                <DetailWorkspaceSection className="shrink-0 last:pb-5">
                    <BusinessFailureState
                        title="后续任务读取失败"
                        description="当前任务可继续办理；完成后可重试读取下一项，或返回队列。"
                        error={queue.error}
                        onRetry={() => void queue.refetch()}
                    />
                </DetailWorkspaceSection>
            )}
            <WorkspaceProcessingLayout
                item={detail.data.item}
                reworkTask={reworkTask}
                allowAutoNext={!approval}
                autoNext={autoNext}
                onAutoNextChange={setAutoNext}
                onBack={back}
                backLabel={backLabel}
                queueNavigation={queueNavigation}
                onRefresh={handleRefresh}
                refreshing={detail.isFetching}
            >
                <WorkspaceTaskDetail
                    key={id}
                    item={detail.data.item}
                    reworkTask={reworkTask}
                    grantedPermissions={profile.data?.permissions ?? []}
                    onDecisionApplied={(result, completedId) =>
                        complete(completedId, result.nextOpenTask?.workItemId)
                    }
                    onTaskCompleted={complete}
                />
            </WorkspaceProcessingLayout>
        </DetailWorkspacePage>
    )
}
