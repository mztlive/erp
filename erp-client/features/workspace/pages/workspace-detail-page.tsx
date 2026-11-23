"use client"

import { workspaceQueueParams } from "@/lib/workspace-queue-routes"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    buildWorkspaceDetailHref,
    workspaceQueueHref,
} from "@/lib/workspace-detail-href"
import {
    readWorkspaceQueueSequence,
    removeFromWorkspaceQueueSequence,
} from "@/lib/workspace-queue-sequence"
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
    WorkspaceHotkeyCheatSheetButton,
    dispatchWorkspaceHotkeyAction,
    useWorkspaceHotkeys,
} from "../hooks/use-workspace-hotkeys"
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
    const queueKind = approval ? "approvals" : "tasks"
    const userId = profile.data?.userid
    const cachedQueueIds = useMemo(
        () => readWorkspaceQueueSequence(userId, queueKind, returnTo),
        [queueKind, returnTo, userId],
    )
    const hasCachedSequence = Boolean(
        cachedQueueIds && cachedQueueIds.includes(id),
    )
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
        canReadQueue && !hasCachedSequence,
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

    // 队列上下项切换（优先使用列表页暂存的轻量 ID 序列即时渲染，无缓存时回退队列查询）
    const queueIds = useMemo(
        () =>
            hasCachedSequence && cachedQueueIds
                ? cachedQueueIds
                : (queue.data?.items.map((entry) => entry.workItemId) ?? []),
        [cachedQueueIds, hasCachedSequence, queue.data?.items],
    )
    const currentIndex = queueIds.indexOf(id)
    const prevId = currentIndex > 0 ? queueIds[currentIndex - 1] : undefined
    const nextId =
        currentIndex >= 0 && currentIndex + 1 < queueIds.length
            ? queueIds[currentIndex + 1]
            : undefined
    const openSibling = useCallback(
        (targetId: string) => {
            router.replace(
                buildWorkspaceDetailHref(queueKind, targetId, {
                    returnTo,
                    autoNext,
                }),
            )
        },
        [autoNext, queueKind, returnTo, router],
    )

    useWorkspaceHotkeys({
        enabled: !completion && Boolean(detail.data),
        onNext: nextId ? () => openSibling(nextId) : undefined,
        onPrev: prevId ? () => openSibling(prevId) : undefined,
        onApprove: () => dispatchWorkspaceHotkeyAction("approve"),
        onReject: () => dispatchWorkspaceHotkeyAction("reject"),
        onMaterials: () => dispatchWorkspaceHotkeyAction("materials"),
        onHistory: () => dispatchWorkspaceHotkeyAction("history"),
        onToggleRail: () => dispatchWorkspaceHotkeyAction("toggle-rail"),
    })

    const queueNavigation = (
        <div className="flex items-center gap-1">
            {currentIndex >= 0 && queueIds.length > 1 ? (
                <>
                    <span className="num mr-1 text-xs text-muted-foreground">
                        第 {currentIndex + 1} / {queueIds.length} 项
                    </span>
                    <Button
                        id="workspace-processing-prev"
                        type="button"
                        variant="ghost"
                        size="xs"
                        className={detailWorkspaceStyles.quietButton}
                        disabled={!prevId}
                        aria-label="上一项 (K)"
                        title="上一项 (K)"
                        onClick={() => prevId && openSibling(prevId)}
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
                        disabled={!nextId}
                        aria-label="下一项 (J)"
                        title="下一项 (J)"
                        onClick={() => nextId && openSibling(nextId)}
                    >
                        下一项
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </>
            ) : null}
            <WorkspaceHotkeyCheatSheetButton
                id="workspace-detail-hotkeys"
                scope="detail"
            />
        </div>
    )

    // 完成通知可能早于队列请求结束；有轻量序列时立即推进，否则待队列查询成功后流转。
    useEffect(() => {
        if (!completion) return
        if (!autoNext) {
            router.replace(returnTo, { scroll: false })
            return
        }
        if (!hasCachedSequence && !queue.isSuccess) return
        const { completedId, preferredId } = completion
        const ids = hasCachedSequence
            ? (cachedQueueIds ?? [])
            : (queue.data?.items.map((entry) => entry.workItemId) ?? [])
        const currentEntryId = ids.includes(completedId) ? completedId : id
        const index = ids.indexOf(currentEntryId)
        const nextTargetId =
            (!approval &&
            preferredId &&
            preferredId !== completedId &&
            preferredId !== currentEntryId
                ? preferredId
                : undefined) ??
            ids
                .slice(index + 1)
                .find((entryId) => entryId !== currentEntryId) ??
            ids.find((entryId) => entryId !== currentEntryId)
        if (hasCachedSequence) {
            removeFromWorkspaceQueueSequence(
                userId,
                queueKind,
                returnTo,
                currentEntryId,
            )
        }
        if (nextTargetId) {
            router.replace(
                buildWorkspaceDetailHref(queueKind, nextTargetId, {
                    returnTo,
                    autoNext: true,
                }),
            )
        } else {
            router.replace(returnTo, { scroll: false })
        }
    }, [
        approval,
        completion,
        autoNext,
        cachedQueueIds,
        hasCachedSequence,
        id,
        queue.isSuccess,
        queue.data,
        queueKind,
        router,
        returnTo,
        userId,
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
                                        {autoNext
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
                allowAutoNext
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
                        complete(
                            completedId,
                            approval
                                ? undefined
                                : result.nextOpenTask?.workItemId,
                        )
                    }
                    onTaskCompleted={complete}
                />
            </WorkspaceProcessingLayout>
        </DetailWorkspacePage>
    )
}
