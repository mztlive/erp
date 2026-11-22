"use client"

import { useEffect, useRef } from "react"
import { Clock3Icon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { ApprovalQueryFeedback } from "@/features/approval-workflow/components/approval-query-feedback"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import {
    displayActorName,
    displayExecutionStatus,
    displayReadableName,
    displayRound,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type {
    ApprovalRoute,
    ApprovalRuntimeInstance,
} from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

type RouteStep = {
    key: string
    name: string
    assignee?: string
    status: string
    tone: StatusTone
    current?: boolean
}

/** 单栏销售审批首屏用一行步骤展示完整冻结路线，历史意见按需查看。 */
export function WorkspaceApprovalRouteSummary({
    instance,
}: {
    instance: ApprovalRuntimeInstance
}) {
    const historyQuery = useApprovalHistoryInfiniteQuery({
        instanceId: instance.id,
        order: "desc",
    })
    const route = historyQuery.data?.pages[0]?.route
    const history = historyQuery.data?.pages.flatMap((page) => page.items) ?? []
    const prefix = `workspace-approval-route-${toAutomationIdSegment(instance.id)}`

    return (
        <section
            aria-label="审批流程"
            className="space-y-1.5 border-t border-border/60 py-2.5"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-3">
                    <h2 className="text-section-title font-semibold tracking-tight">
                        审批流程
                    </h2>
                    <span className="text-xs text-muted-foreground">
                        {displayRound(
                            route?.roundNo ?? instance.currentRoundNo,
                        )}
                    </span>
                </div>
                <Dialog>
                    <DialogTrigger
                        id={`${prefix}-history`}
                        render={<Button variant="ghost" size="sm" />}
                    >
                        查看审批记录
                    </DialogTrigger>
                    <DialogContent
                        closeButtonId={`${prefix}-history-close`}
                        className="flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-2xl"
                    >
                        <DialogHeader className="shrink-0 pr-8">
                            <DialogTitle>审批记录</DialogTitle>
                            <DialogDescription>
                                查看各轮审批的处理人、时间和审批意见。
                            </DialogDescription>
                        </DialogHeader>
                        <div className="min-h-0 flex-1 space-y-4 overflow-auto">
                            {historyQuery.isPending ? (
                                <p
                                    role="status"
                                    className="text-xs text-muted-foreground"
                                >
                                    正在加载审批记录…
                                </p>
                            ) : history.length > 0 || !historyQuery.isError ? (
                                <ExecutionHistory
                                    id={`${prefix}-executions`}
                                    items={history}
                                    presentation="timeline"
                                    hasMore={historyQuery.hasNextPage}
                                    loadingMore={
                                        historyQuery.isFetchingNextPage
                                    }
                                    onLoadMore={() =>
                                        void historyQuery.fetchNextPage()
                                    }
                                />
                            ) : null}
                            <ApprovalQueryFeedback
                                surface={`${prefix}-dialog`}
                                instanceId={instance.id}
                                historyQuery={historyQuery}
                            />
                        </div>
                    </DialogContent>
                </Dialog>
            </div>
            {route?.nodes.length ? (
                <ApprovalRouteStepper
                    id={`${prefix}-steps`}
                    route={route}
                    instanceStatus={instance.status}
                />
            ) : historyQuery.isPending ? (
                <p role="status" className="text-xs text-muted-foreground">
                    正在加载审批流程…
                </p>
            ) : !historyQuery.isError ? (
                <BusinessFailureState
                    id={`${prefix}-missing`}
                    title="完整审批流程暂不可用，请刷新重试"
                    onRetry={() => void historyQuery.refetch()}
                />
            ) : null}
            {historyQuery.isError ? (
                <BusinessFailureState
                    id={`${prefix}-failure`}
                    title="审批流程读取失败，请重试；已显示的流程可能不是最新状态"
                    error={historyQuery.error}
                    onRetry={() => void historyQuery.refetch()}
                />
            ) : null}
        </section>
    )
}

function ApprovalRouteStepper({
    id,
    route,
    instanceStatus,
}: {
    id: string
    route: ApprovalRoute
    instanceStatus: string
}) {
    const steps = routeSteps(route, instanceStatus)
    const focusKey =
        steps.find((step) => step.current)?.key ??
        (isTerminalStatus(instanceStatus) ? "done" : undefined)
    const listRef = useRef<HTMLOListElement>(null)
    const focusRef = useRef<HTMLLIElement>(null)
    useEffect(() => {
        const list = listRef.current
        const step = focusRef.current
        if (!list || !step) return
        const listRect = list.getBoundingClientRect()
        const stepRect = step.getBoundingClientRect()
        const delta =
            stepRect.left -
            listRect.left -
            (listRect.width - stepRect.width) / 2
        list.scrollLeft += delta
    }, [focusKey, route.roundNo])
    const opinions = route.nodes.flatMap((node) => {
        const failed = node.result === "REJECTED" || node.result === "BLOCKED"
        const reason = node.execution?.decisionReason?.trim()
        if (!failed || !reason) return []
        const name =
            displayReadableName(node.name, node.key) || "审批节点未标注"
        return [{ key: node.key, name, reason }]
    })

    return (
        <div className="space-y-1">
            <ol
                id={id}
                ref={listRef}
                aria-label="完整审批人链条"
                className="flex min-w-0 items-center overflow-x-auto overscroll-x-contain py-0.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
            >
                {steps.map((step, index) => (
                    <li
                        key={step.key}
                        ref={step.key === focusKey ? focusRef : undefined}
                        aria-current={step.current ? "step" : undefined}
                        className="flex shrink-0 items-center whitespace-nowrap text-xs"
                    >
                        {index > 0 ? (
                            <span
                                aria-hidden="true"
                                className="mx-2 h-px w-3 bg-border"
                            />
                        ) : null}
                        <StatusBadge
                            appearance="text"
                            tone={step.tone}
                            icon={
                                step.tone === "warning" ? Clock3Icon : undefined
                            }
                            label={step.status}
                        />
                        <span
                            className={cn(
                                "ml-1.5",
                                step.current
                                    ? "font-semibold text-foreground"
                                    : step.tone === "neutral"
                                      ? "text-muted-foreground"
                                      : "text-foreground",
                            )}
                        >
                            {step.name}
                        </span>
                        {step.assignee ? (
                            <span className="ml-1 text-muted-foreground">
                                · {step.assignee}
                            </span>
                        ) : null}
                    </li>
                ))}
            </ol>
            {opinions.map((opinion) => (
                <p
                    key={opinion.key}
                    className="truncate text-xs text-destructive"
                    title={`${opinion.name}：${opinion.reason}`}
                >
                    {opinion.name}：{opinion.reason}
                </p>
            ))}
        </div>
    )
}

function routeSteps(route: ApprovalRoute, instanceStatus: string): RouteStep[] {
    return [
        {
            key: "submitted",
            name: "提交申请",
            status: "已提交",
            tone: "success",
        },
        ...route.nodes.map((node) => {
            const presentation = nodePresentation(node.result)
            return {
                key: node.key,
                name:
                    displayReadableName(node.name, node.key) ||
                    "审批节点未标注",
                assignee: displayActorName(node.assigneeName) || "审批人未标注",
                status: presentation.status,
                tone: presentation.tone,
                current: presentation.current,
            }
        }),
        closingStep(instanceStatus),
    ]
}

function nodePresentation(result: string): {
    status: string
    tone: StatusTone
    current: boolean
} {
    switch (result) {
        case "APPROVED":
            return { status: "已通过", tone: "success", current: false }
        case "REJECTED":
            return { status: "已驳回", tone: "destructive", current: false }
        case "BLOCKED":
            return { status: "受阻", tone: "destructive", current: true }
        case "CANCELLED":
            return { status: "已撤回", tone: "void", current: false }
        case "PENDING":
            return { status: "待审批", tone: "neutral", current: false }
        case "NOT_REACHED":
            return { status: "未执行", tone: "neutral", current: false }
        case "ACTIVE":
            return { status: "当前审批", tone: "warning", current: true }
        case "SUPERSEDED":
            return {
                status: "已由后续轮次替代",
                tone: "neutral",
                current: false,
            }
        default:
            return {
                status: displayExecutionStatus(result),
                tone: "info",
                current: false,
            }
    }
}

function closingStep(instanceStatus: string): RouteStep {
    switch (instanceStatus) {
        case "APPROVED":
            return {
                key: "done",
                name: "审批完成",
                status: "已通过",
                tone: "success",
            }
        case "REJECTED":
            return {
                key: "done",
                name: "审批驳回",
                status: "已驳回",
                tone: "destructive",
            }
        case "CANCELLED":
            return {
                key: "done",
                name: "审批撤回",
                status: "已撤回",
                tone: "void",
            }
        default:
            return {
                key: "done",
                name: "审批完成",
                status: "待完成",
                tone: "neutral",
            }
    }
}

function isTerminalStatus(status: string): boolean {
    return (
        status === "APPROVED" || status === "REJECTED" || status === "CANCELLED"
    )
}
