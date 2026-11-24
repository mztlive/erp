"use client"

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"
import {
    BanIcon,
    CheckIcon,
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    ChevronUpIcon,
    CircleAlertIcon,
    CircleDashedIcon,
    Clock3Icon,
    HistoryIcon,
    type LucideIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import { formatDateTime, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { getApprovalNodeStatusMeta } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import { displayActorName, displayReadableName } from "../display"
import type { ApprovalRoute } from "../types"

export type ApprovalRouteStepperProps = Readonly<{
    route: ApprovalRoute
    instanceStatus?: string
    /** 上轮或最新驳回说明（当前轮未产生驳回节点时展示）。 */
    latestRejection?: string | null
    latestRejectionByName?: string | null
    /** 点击驳回条跳转/锚定到右侧审计轨迹中的对应记录。 */
    onJumpToHistory?: (executionId?: string) => void
    /** 外层正文已展示驳回横幅时隐藏步进条下方重复驳回条。 */
    hideRejectionBanner?: boolean
    /** 步进条右侧内联轻动作（供分栏检查器与抽屉复用，避免多叠一层工具栏）。 */
    trailingActions?: ReactNode
    idPrefix?: string
    className?: string
}>

type StepItem = Readonly<{
    key: string
    name: string
    assignee: string
    statusLabel: string
    tone: StatusTone
    current: boolean
    Icon: LucideIcon
    decidedIso?: string
    decidedShortLabel?: string
}>

type RejectionSummary = Readonly<{
    key: string
    executionId?: string
    prefix: string
    reason: string
    isCurrentRound: boolean
}>

function formatCompactMonthDay(
    secs?: number | null,
): { iso: string; label: string } | undefined {
    const iso = unixSecondsToIsoOrEmpty(secs)
    if (!iso) return undefined
    const formatted = formatDateTime(iso, "monthDay")
    if (!formatted || formatted === "—") return undefined
    return {
        iso,
        label: formatted.replace("/", "-"),
    }
}

function nodeIconForResult(result: string, tone: StatusTone): LucideIcon {
    if (result === "APPROVED") return CheckIcon
    if (result === "REJECTED" || result === "BLOCKED") return CircleAlertIcon
    if (result === "ACTIVE") return Clock3Icon
    if (result === "CANCELLED") return BanIcon
    if (tone === "success") return CheckIcon
    if (tone === "destructive") return CircleAlertIcon
    if (tone === "warning") return Clock3Icon
    if (tone === "void") return BanIcon
    return CircleDashedIcon
}

function nodeChipClass(tone: StatusTone, current: boolean): string {
    if (current && tone === "destructive") {
        return "border-destructive-border bg-destructive-soft/80 text-destructive-soft-foreground"
    }
    if (current) {
        return "border-warning-border bg-warning-soft/80 text-warning-soft-foreground"
    }
    return "border-transparent bg-transparent text-foreground"
}

function nodeIconClass(tone: StatusTone): string {
    switch (tone) {
        case "success":
            return "border-success-border bg-success-soft text-success-soft-foreground"
        case "destructive":
            return "border-destructive-border bg-destructive-soft text-destructive-soft-foreground"
        case "warning":
            return "border-warning-border bg-warning-soft text-warning-soft-foreground"
        case "info":
            return "border-info-border bg-info-soft text-info-soft-foreground"
        case "void":
            return "border-neutral-border bg-neutral-soft text-neutral-soft-foreground"
        default:
            return "border-border bg-muted/60 text-muted-foreground"
    }
}

/**
 * 紧凑单行横向审批步进条（~44px），展示当前轮次冻结审批节点流，
 * 已决节点通过 `formatDateTime` 展示 `MM-DD HH:mm` 精简时间，并附带单行可展开驳回警示条。
 */
export function ApprovalRouteStepper({
    route,
    instanceStatus,
    latestRejection,
    latestRejectionByName,
    onJumpToHistory,
    hideRejectionBanner = false,
    trailingActions,
    idPrefix = "approval-route-stepper",
    className,
}: ApprovalRouteStepperProps) {
    const listRef = useRef<HTMLOListElement>(null)
    const focusRef = useRef<HTMLLIElement>(null)
    const [overflow, setOverflow] = useState({ left: false, right: false })
    const [rejectionExpanded, setRejectionExpanded] = useState(false)

    const steps: StepItem[] = route.nodes.map((node) => {
        const isNotReached = node.result === "NOT_REACHED"
        const meta = isNotReached
            ? { label: "未执行", tone: "neutral" as const }
            : getApprovalNodeStatusMeta(node.result)
        const current = node.result === "ACTIVE" || node.result === "BLOCKED"
        const decided = formatCompactMonthDay(node.execution?.decidedAt)
        return {
            key: node.key,
            name: displayReadableName(node.name, node.key) || "审批节点未标注",
            assignee: displayActorName(node.assigneeName) || "审批人未标注",
            statusLabel: meta.label,
            tone: meta.tone,
            current,
            Icon: nodeIconForResult(node.result, meta.tone),
            decidedIso: decided?.iso,
            decidedShortLabel: decided?.label,
        }
    })

    const focusKey = steps.find((step) => step.current)?.key

    const updateOverflow = useCallback(() => {
        const list = listRef.current
        if (!list) return
        const left = list.scrollLeft > 4
        const right = list.scrollLeft + list.clientWidth < list.scrollWidth - 4
        setOverflow((current) =>
            current.left === left && current.right === right
                ? current
                : { left, right },
        )
    }, [])

    useEffect(() => {
        const list = listRef.current
        const step = focusRef.current
        if (list && step) {
            const listRect = list.getBoundingClientRect()
            const stepRect = step.getBoundingClientRect()
            const delta =
                stepRect.left -
                listRect.left -
                (listRect.width - stepRect.width) / 2
            list.scrollLeft += delta
        }
        updateOverflow()
    }, [focusKey, route.roundNo, updateOverflow])

    useEffect(() => {
        const list = listRef.current
        if (!list) return
        updateOverflow()
        const observer =
            typeof ResizeObserver !== "undefined"
                ? new ResizeObserver(updateOverflow)
                : null
        observer?.observe(list)
        return () => observer?.disconnect()
    }, [steps.length, updateOverflow])

    const scrollStep = (direction: -1 | 1) => {
        const list = listRef.current
        if (!list) return
        list.scrollBy({
            left: direction * Math.max(180, list.clientWidth * 0.6),
            behavior: "smooth",
        })
    }

    const currentRoundFailure = route.nodes.find(
        (node) =>
            (node.result === "REJECTED" || node.result === "BLOCKED") &&
            Boolean(node.execution?.decisionReason?.trim()),
    )

    const rejection: RejectionSummary | null = currentRoundFailure
        ? {
              key: currentRoundFailure.key,
              executionId: currentRoundFailure.execution?.executionId,
              prefix: `${
                  displayReadableName(
                      currentRoundFailure.name,
                      currentRoundFailure.key,
                  ) || "审批节点"
              }${
                  currentRoundFailure.result === "BLOCKED" ? "（受阻）" : "驳回"
              }`,
              reason:
                  currentRoundFailure.execution?.decisionReason?.trim() ?? "",
              isCurrentRound: true,
          }
        : latestRejection?.trim()
          ? {
                key: "latest-rejection",
                prefix: latestRejectionByName
                    ? `${instanceStatus === "REJECTED" ? "驳回原因" : "上轮驳回"}（${displayActorName(latestRejectionByName)}）`
                    : instanceStatus === "REJECTED"
                      ? "驳回原因"
                      : "上轮驳回",
                reason: latestRejection.trim(),
                isCurrentRound: instanceStatus === "REJECTED",
            }
          : null

    return (
        <div
            data-slot="approval-route-stepper"
            className={cn("min-w-0 space-y-1.5", className)}
        >
            <div className="relative flex h-11 min-w-0 items-center gap-1 border-b border-border/60 bg-card px-2">
                {overflow.left ? (
                    <Button
                        id={`${idPrefix}-scroll-prev`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="向左查看审批节点"
                        className="shrink-0 text-muted-foreground hover:text-foreground"
                        onClick={() => scrollStep(-1)}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                    </Button>
                ) : null}
                <ol
                    id={`${idPrefix}-list`}
                    ref={listRef}
                    aria-label="当前轮次审批步进条"
                    onScroll={updateOverflow}
                    className="flex min-w-0 flex-1 items-center overflow-x-auto overscroll-x-contain [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
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
                                    className="mx-1.5 h-px w-3.5 shrink-0 bg-border"
                                />
                            ) : null}
                            <span
                                title={`${step.name} · ${step.assignee} · ${step.statusLabel}${step.decidedShortLabel ? ` · ${step.decidedShortLabel}` : ""}`}
                                className={cn(
                                    "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 transition-colors",
                                    nodeChipClass(step.tone, step.current),
                                )}
                            >
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "flex size-4 shrink-0 items-center justify-center rounded-full border",
                                        nodeIconClass(step.tone),
                                    )}
                                >
                                    <step.Icon className="size-2.5" />
                                </span>
                                <span className="sr-only">
                                    {step.statusLabel}
                                </span>
                                <span
                                    className={cn(
                                        step.current
                                            ? "font-semibold text-foreground"
                                            : step.tone === "neutral"
                                              ? "text-muted-foreground"
                                              : "font-medium text-foreground",
                                    )}
                                >
                                    {step.name}
                                </span>
                                <span className="text-muted-foreground">
                                    {step.assignee}
                                </span>
                                {step.decidedShortLabel ? (
                                    <time
                                        dateTime={step.decidedIso}
                                        className="num text-tiny text-muted-foreground"
                                    >
                                        {step.decidedShortLabel}
                                    </time>
                                ) : null}
                            </span>
                        </li>
                    ))}
                </ol>
                {overflow.right ? (
                    <Button
                        id={`${idPrefix}-scroll-next`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="向右查看审批节点"
                        className="shrink-0 text-muted-foreground hover:text-foreground"
                        onClick={() => scrollStep(1)}
                    >
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                ) : null}
                {trailingActions ? (
                    <>
                        <span
                            aria-hidden="true"
                            className="mx-1 h-4 w-px shrink-0 bg-border/70"
                        />
                        <div className="flex shrink-0 items-center gap-1">
                            {trailingActions}
                        </div>
                    </>
                ) : null}
            </div>

            {!hideRejectionBanner && rejection ? (
                <div
                    role="status"
                    className={cn(
                        "flex items-start gap-2 rounded-md border px-2.5 py-1.5 text-xs",
                        rejection.isCurrentRound
                            ? "border-destructive-border bg-destructive-soft/80 text-destructive-soft-foreground"
                            : "border-warning-border bg-warning-soft/80 text-warning-soft-foreground",
                    )}
                >
                    <CircleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                        <p
                            className={cn(
                                rejectionExpanded
                                    ? "whitespace-pre-wrap break-words"
                                    : "truncate",
                            )}
                            title={`${rejection.prefix}：${rejection.reason}`}
                        >
                            <span className="font-medium">
                                {rejection.prefix}：
                            </span>
                            {rejection.reason}
                        </p>
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                        <Button
                            id={`${idPrefix}-rejection-toggle`}
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            aria-expanded={rejectionExpanded}
                            aria-label={
                                rejectionExpanded
                                    ? "收起驳回详情"
                                    : "展开驳回详情"
                            }
                            className="text-current hover:bg-background/40"
                            onClick={() =>
                                setRejectionExpanded((expanded) => !expanded)
                            }
                        >
                            {rejectionExpanded ? (
                                <ChevronUpIcon aria-hidden="true" />
                            ) : (
                                <ChevronDownIcon aria-hidden="true" />
                            )}
                        </Button>
                        {onJumpToHistory ? (
                            <Button
                                id={`${idPrefix}-rejection-jump`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="h-control-xs px-1.5 text-current hover:bg-background/40"
                                onClick={() =>
                                    onJumpToHistory(rejection.executionId)
                                }
                            >
                                <HistoryIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                定位历史
                            </Button>
                        ) : null}
                    </div>
                </div>
            ) : null}
        </div>
    )
}
