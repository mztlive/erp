"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
    AlertTriangleIcon,
    ArrowUpRightIcon,
    RefreshCwIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge } from "@/components/ui/status-badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { AUTO_NEXT_LABEL } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import { workspaceHost } from "@/lib/workspace-tabs"
import {
    useWorkspaceApprovalDetailQuery,
    useWorkspaceTaskDetailQuery,
    workspaceHomeKeys,
} from "../hooks/queries"
import { workspaceTaskPresentation } from "../lib/task-presentation"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceMoney } from "./workspace-money"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"
import { WorkspaceTaskSurface } from "./workspace-task-surface"
import { WorkspaceTaskSurfaceBoundary } from "./workspace-task-surface-boundary"

const AUTO_NEXT_STORAGE_KEY = "erp.workspace.autoNext"
const AUTO_NEXT_EVENT = "erp-workspace-auto-next-change"

/** 超宽复杂建单 / 长表单事项在分栏检查器中展示高密度摘要卡并引导进入完整工作区页签。 */
export const HEAVY_FULLPAGE_WORK_ITEM_TYPES = new Set<string>([
    "SALES_INQUIRY_PROCESSING",
    "PROCUREMENT_ORDER_CREATION",
    "SUPPLIER_PORTAL_REVIEW",
])

export function isHeavyFullPageWorkItem(item: WorkspaceWorkItem): boolean {
    return HEAVY_FULLPAGE_WORK_ITEM_TYPES.has(item.workItemType)
}

function readStoredAutoNext(): boolean {
    if (typeof window === "undefined") return false
    try {
        return window.localStorage.getItem(AUTO_NEXT_STORAGE_KEY) === "1"
    } catch {
        return false
    }
}

function writeStoredAutoNext(next: boolean): void {
    if (typeof window === "undefined") return
    try {
        window.localStorage.setItem(AUTO_NEXT_STORAGE_KEY, next ? "1" : "0")
    } catch {
        // 隐私模式下忽略本地存储写入异常
    }
    window.dispatchEvent(new Event(AUTO_NEXT_EVENT))
}

function useStoredAutoNext(): readonly [boolean, (next: boolean) => void] {
    const subscribe = React.useCallback((listener: () => void) => {
        if (typeof window === "undefined") return () => {}
        const onStorage = (event: StorageEvent) => {
            if (event.key === null || event.key === AUTO_NEXT_STORAGE_KEY) {
                listener()
            }
        }
        window.addEventListener(AUTO_NEXT_EVENT, listener)
        window.addEventListener("storage", onStorage)
        return () => {
            window.removeEventListener(AUTO_NEXT_EVENT, listener)
            window.removeEventListener("storage", onStorage)
        }
    }, [])

    const value = React.useSyncExternalStore(
        subscribe,
        readStoredAutoNext,
        () => false,
    )
    return [value, writeStoredAutoNext] as const
}

export type WorkspaceTaskInspectorProps = Readonly<{
    item: WorkspaceWorkItem
    /** 打开完整独立工作区页签。 */
    onOpenFullPage: () => void
    /** 任务完成、转交或关闭后的回调；携带当前 `autoNext` 偏好供上层决定推进或收起。 */
    onCompleted?: (
        completedWorkItemId: string,
        preferredWorkItemId: string | undefined,
        autoNext: boolean,
    ) => void
    /** `"pane"` 表示宽屏右栏检查器；`"sheet"` 表示窄屏或通栏快览抽屉。 */
    presentation: "pane" | "sheet"
}>

function HeavyTaskSummaryCard({
    item,
    onOpenFullPage,
    onCompleted,
    presentation,
}: {
    item: WorkspaceWorkItem
    onOpenFullPage: () => void
    onCompleted?: (completedId: string) => void
    presentation: "pane" | "sheet"
}) {
    const presentationMeta = workspaceTaskPresentation(item)
    const blocked = isBlockedWorkItem(item)
    const overdue = item.status === "OPEN" && item.dueBucket === "overdue"
    const segment = toAutomationIdSegment(item.workItemId)
    const sections = (item.summarySections ?? []).filter(
        (section) => section.value && section.value !== "—",
    )
    const briefLines = item.briefLines ?? []
    const padClass = presentation === "sheet" ? "px-sheet-inline" : "px-5"

    return (
        <div
            data-slot="workspace-heavy-task-summary"
            className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div
                className={cn(
                    "min-h-0 flex-1 space-y-5 overflow-y-auto overscroll-contain py-5",
                    padClass,
                )}
            >
                {overdue || presentationMeta.summary ? (
                    <div className="space-y-2 border-b border-border/60 pb-4">
                        {overdue ? (
                            <div className="flex flex-wrap items-center gap-1.5">
                                <StatusBadge
                                    label="已超期"
                                    tone="destructive"
                                />
                            </div>
                        ) : null}
                        {presentationMeta.summary ? (
                            <p className="text-body-compact text-muted-foreground">
                                {presentationMeta.summary}
                            </p>
                        ) : null}
                    </div>
                ) : null}

                {blocked && item.actionBlockers.length > 0 ? (
                    <Alert variant="warning">
                        <AlertTriangleIcon aria-hidden="true" />
                        <AlertTitle>当前存在办理限制</AlertTitle>
                        <AlertDescription>
                            {item.actionBlockers
                                .map((blocker) => blocker.message)
                                .join("；")}
                        </AlertDescription>
                    </Alert>
                ) : null}

                <DetailWorkspaceFacts columns={2} ariaLabel="核心事项摘要">
                    {presentationMeta.amount ? (
                        <DetailWorkspaceFact
                            label={presentationMeta.amount.label}
                            numeric
                            value={
                                <WorkspaceMoney
                                    value={presentationMeta.amount.value}
                                    display="exact"
                                />
                            }
                        />
                    ) : null}
                    <DetailWorkspaceFact
                        label="当前责任"
                        value={
                            [item.ownerRoleLabel, item.ownerUserLabel]
                                .filter(Boolean)
                                .join(" · ") || "—"
                        }
                    />
                    <DetailWorkspaceFact
                        label="进入时间"
                        numeric
                        value={item.enteredAtLabel}
                    />
                    <DetailWorkspaceFact
                        label="截止时间"
                        numeric
                        value={item.dueAtLabel || "未设截止时间"}
                    />
                </DetailWorkspaceFacts>

                {sections.length > 0 ? (
                    <div className="space-y-3 border-t border-border/60 pt-4">
                        <h3 className="text-xs font-semibold text-muted-foreground">
                            关键业务信息
                        </h3>
                        <DetailWorkspaceFacts columns={2}>
                            {sections.slice(0, 8).map((section) => (
                                <DetailWorkspaceFact
                                    key={`${section.label}:${section.value}`}
                                    label={section.label}
                                    numeric={section.numeric}
                                    value={section.value}
                                />
                            ))}
                        </DetailWorkspaceFacts>
                    </div>
                ) : null}

                {briefLines.length > 0 ? (
                    <div className="space-y-2.5 border-t border-border/60 pt-4">
                        <h3 className="text-xs font-semibold text-muted-foreground">
                            明细摘要
                        </h3>
                        <ul className="divide-y divide-border/60">
                            {briefLines.slice(0, 5).map((line, index) => (
                                <li
                                    key={`${line.title}:${index}`}
                                    className="flex items-baseline justify-between gap-3 py-2 text-body-compact first:pt-0 last:pb-0"
                                >
                                    <span className="min-w-0 truncate text-foreground">
                                        {line.title}
                                    </span>
                                    {line.quantity ? (
                                        <span className="num shrink-0 text-xs text-muted-foreground">
                                            {line.quantity}
                                        </span>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                        {(item.briefMoreCount ?? 0) > 0 ? (
                            <p className="num text-xs text-muted-foreground">
                                另有 {item.briefMoreCount}{" "}
                                行明细，请在完整作业面查看与编辑。
                            </p>
                        ) : null}
                    </div>
                ) : null}

                {item.nextActionHint || item.impactSummary ? (
                    <div className="space-y-1.5 border-t border-border/60 pt-4 text-xs text-muted-foreground">
                        {item.nextActionHint ? (
                            <p>{item.nextActionHint}</p>
                        ) : null}
                        {item.impactSummary ? (
                            <p>{item.impactSummary}</p>
                        ) : null}
                    </div>
                ) : null}

                <div className="rounded-md border border-border/70 bg-muted/35 px-3.5 py-2.5 text-xs text-muted-foreground">
                    该事项包含多列宽表或复杂建单表单，建议进入完整工作区页签专注办理。
                </div>
            </div>

            <footer
                data-slot="workspace-inspector-footer"
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/60 bg-card py-3 shadow-footer",
                    padClass,
                )}
            >
                <div className="flex flex-wrap items-center gap-2">
                    <WorkspaceTaskResponsibilityActions
                        item={item}
                        onCompleted={onCompleted}
                    />
                </div>
                <Button
                    id={`workspace-inspector-enter-full-${segment}`}
                    type="button"
                    size="sm"
                    onClick={onOpenFullPage}
                >
                    进入完整作业面
                    <ArrowUpRightIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                    />
                </Button>
            </footer>
        </div>
    )
}

/**
 * 工作台原位任务检查器：运行于 `WorkspaceSplitQueue` 右分栏或窄屏 `TaskActionSheet` 内，
 * 直接共享当前页签的 TanStack Query 缓存（零 iframe 冷启动）。
 *
 * - 重型建单/超宽表单（`SALES_INQUIRY_PROCESSING` / `PROCUREMENT_ORDER_CREATION` / `SUPPLIER_PORTAL_REVIEW`）
 *   展示高密度摘要卡与「进入完整作业面」直达主按钮；
 * - 常规审批、退回重办与标准执行事项直接挂载 `WorkspaceTaskSurface`，办理完成后按 `autoNext` 推进队列。
 */
export function WorkspaceTaskInspector({
    item,
    onOpenFullPage,
    onCompleted,
    presentation,
}: WorkspaceTaskInspectorProps) {
    const queryClient = useQueryClient()
    const profileQuery = useAccountProfileQuery()
    const profile = profileQuery.data
    const [autoNext, setAutoNext] = useStoredAutoNext()

    const isHeavy = isHeavyFullPageWorkItem(item)
    const isApprovalInstance = item.workItemType === "APPROVAL_INSTANCE"

    const taskDetailQuery = useWorkspaceTaskDetailQuery(
        !isHeavy && !isApprovalInstance ? item.workItemId : undefined,
        profile,
        !isHeavy && !isApprovalInstance,
    )
    const approvalDetailQuery = useWorkspaceApprovalDetailQuery(
        !isHeavy && isApprovalInstance ? item.workItemId : undefined,
        profile,
    )
    const detailQuery = isApprovalInstance
        ? approvalDetailQuery
        : taskDetailQuery

    const notifyQueueCompleted = React.useCallback(
        (completedId: string, preferredId?: string) => {
            workspaceHost()?.changed(window)
            void queryClient.invalidateQueries({
                queryKey: workspaceHomeKeys.all,
            })
            onCompleted?.(completedId, preferredId, autoNext)
        },
        [autoNext, onCompleted, queryClient],
    )

    const handleDecisionApplied = React.useCallback(
        (view: ApprovalCommandView, completedId: string) => {
            notifyQueueCompleted(completedId, view.nextOpenTask?.workItemId)
        },
        [notifyQueueCompleted],
    )

    const handleRefresh = React.useCallback(() => {
        void Promise.all([
            detailQuery.refetch(),
            queryClient.invalidateQueries({ queryKey: ["approval"] }),
            queryClient.invalidateQueries({
                queryKey: ["purchase-payment-requests"],
            }),
            queryClient.invalidateQueries({
                queryKey: ["workspace-invoice-review"],
            }),
        ])
    }, [detailQuery, queryClient])

    const padClass = presentation === "sheet" ? "px-sheet-inline" : "px-5"
    const segment = toAutomationIdSegment(item.workItemId)

    if (isHeavy) {
        return (
            <HeavyTaskSummaryCard
                item={item}
                onOpenFullPage={onOpenFullPage}
                onCompleted={(completedId) => notifyQueueCompleted(completedId)}
                presentation={presentation}
            />
        )
    }

    if (profileQuery.isError || detailQuery.isError) {
        return (
            <div
                className={cn(
                    "flex min-h-0 flex-1 flex-col justify-center py-6",
                    padClass,
                )}
            >
                <BusinessFailureState
                    idPrefix={`workspace-inspector-failure-${segment}`}
                    title={
                        isApprovalInstance
                            ? "审批详情读取失败"
                            : "任务详情读取失败"
                    }
                    error={profileQuery.error ?? detailQuery.error}
                    onRetry={() =>
                        void (profileQuery.isError
                            ? profileQuery.refetch()
                            : detailQuery.refetch())
                    }
                    action={
                        <Button
                            id={`workspace-inspector-error-open-full-${segment}`}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onOpenFullPage}
                        >
                            在完整页面打开
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    }
                />
            </div>
        )
    }

    if (profileQuery.isPending || !detailQuery.data) {
        return (
            <div
                aria-busy="true"
                className={cn(
                    "flex min-h-0 flex-1 flex-col gap-4 py-5",
                    padClass,
                )}
            >
                <span role="status" className="sr-only">
                    正在读取事项详情…
                </span>
                <div className="space-y-2 border-b border-border/60 pb-4">
                    <Skeleton className="h-4 w-28" />
                    <Skeleton className="h-5 w-60" />
                    <Skeleton className="h-3.5 w-44" />
                </div>
                <div className="grid grid-cols-2 gap-4">
                    <Skeleton className="h-12 w-full" />
                    <Skeleton className="h-12 w-full" />
                </div>
                <Skeleton className="h-40 w-full" />
            </div>
        )
    }

    const resolvedItem = detailQuery.data.item
    const reworkTask = detailQuery.data.reworkTask
    const actionable = (reworkTask ?? resolvedItem).allowedActions.some(
        (action) => ["PROCESS", "APPROVE", "REJECT"].includes(action),
    )

    return (
        <div
            data-slot="workspace-task-inspector"
            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div
                data-slot="workspace-task-inspector-controls"
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/60 py-1.5 text-xs",
                    padClass,
                )}
            >
                {actionable ? (
                    <label
                        htmlFor={`workspace-inspector-auto-next-${segment}`}
                        className="flex cursor-pointer items-center gap-1.5 text-muted-foreground select-none hover:text-foreground"
                    >
                        <NativeCheckbox
                            id={`workspace-inspector-auto-next-${segment}`}
                            checked={autoNext}
                            onCheckedChange={(checked) =>
                                setAutoNext(checked === true)
                            }
                        />
                        <span>{AUTO_NEXT_LABEL}</span>
                    </label>
                ) : (
                    <span className="text-muted-foreground">
                        原位核对模式（共享当前会话缓存）
                    </span>
                )}
                <Button
                    id={`workspace-inspector-refresh-${segment}`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-control-xs px-2 text-xs text-muted-foreground hover:text-foreground"
                    disabled={detailQuery.isFetching}
                    onClick={handleRefresh}
                >
                    <RefreshCwIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    刷新详情
                </Button>
            </div>

            <section
                aria-label="当前工作台任务"
                data-slot="workspace-task-surface"
                className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
            >
                <WorkspaceTaskSurfaceBoundary
                    workItemId={resolvedItem.workItemId}
                >
                    <WorkspaceTaskSurface
                        item={resolvedItem}
                        reworkTask={reworkTask}
                        grantedPermissions={profile?.permissions ?? []}
                        onDecisionApplied={handleDecisionApplied}
                        onTaskCompleted={notifyQueueCompleted}
                        inspectorMode
                    />
                </WorkspaceTaskSurfaceBoundary>
            </section>
        </div>
    )
}
