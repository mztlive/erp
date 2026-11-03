"use client"

import {
    CheckIcon,
    CircleAlertIcon,
    Clock3Icon,
    MinusIcon,
    type LucideIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { LoadingButton } from "@/components/ui/loading-button"
import { ApprovalQueryFeedback } from "@/features/approval-workflow/components/approval-query-feedback"
import { ApprovalRoute } from "@/features/approval-workflow/components/approval-route"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import {
    displayActorName,
    displayExecutionStatus,
    displayReadableName,
    displayRound,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { WorkspaceDescriptionText } from "./workspace-description-text"

type ApprovalStep = {
    key: string
    label: string
    detail: string
    state: string
    icon: LucideIcon
    current?: boolean
    round?: string
    opinion?: string
}

/** 按执行记录展示审批人和意见；意见颜色只取对应执行的结果。 */
export function WorkspaceApprovalProgress({
    instance,
    compact = false,
    vertical = false,
}: {
    compact?: boolean
    vertical?: boolean
    instance: ApprovalRuntimeInstance
}) {
    const approved = instance.status === "APPROVED"
    const cancelled = instance.status === "CANCELLED"
    const rejected = instance.status === "REJECTED"
    const blocked = instance.status === "BLOCKED"
    const running = instance.status === "RUNNING"
    const active = running || blocked
    const assignee = displayActorName(
        instance.currentAssigneeName,
        instance.currentAssignee,
    )
    const version =
        instance.processVersion == null
            ? ""
            : String(instance.processVersion).trim()
    const processName = displayReadableName(instance.processName)
    const historyQuery = useApprovalHistoryInfiniteQuery({
        instanceId: instance.id,
        order: "desc",
    })
    const route = historyQuery.data?.pages[0]?.route
    const history = (
        historyQuery.data?.pages.flatMap((page) => page.items) ?? []
    )
        .slice()
        .sort(
            (left, right) =>
                left.roundNo - right.roundNo ||
                left.executionNo - right.executionNo,
        )
    const showVertical = vertical || history.length > 1
    const processingSteps: ApprovalStep[] = history.length
        ? history.map((entry) => ({
              key: entry.executionId,
              label:
                  displayReadableName(entry.nodeName, entry.nodeKey) ||
                  "审批处理",
              detail: [
                  displayActorName(entry.assigneeName, entry.decidedBy) ||
                      "审批人未标注",
                  displayExecutionStatus(entry.result),
              ].join(" · "),
              state:
                  entry.result === "APPROVED"
                      ? "approved"
                      : entry.result === "REJECTED"
                        ? "rejected"
                        : entry.result === "BLOCKED"
                          ? "blocked"
                          : entry.result === "ACTIVE"
                            ? "active"
                            : "muted",
              icon:
                  entry.result === "APPROVED"
                      ? CheckIcon
                      : entry.result === "REJECTED" ||
                          entry.result === "BLOCKED"
                        ? CircleAlertIcon
                        : entry.result === "ACTIVE"
                          ? Clock3Icon
                          : MinusIcon,
              current: active && entry.result === "ACTIVE",
              round:
                  instance.currentRoundNo > 1 || entry.roundNo > 1
                      ? displayRound(entry.roundNo)
                      : undefined,
              opinion: entry.decisionReason,
          }))
        : [
              {
                  key: "processing",
                  label: active
                      ? displayReadableName(
                            instance.currentNodeName,
                            instance.currentNode,
                        ) || "审批处理"
                      : "审批处理",
                  detail: active
                      ? [
                            assignee || "审批人待确认",
                            blocked ? "受阻" : "审批中",
                        ].join(" · ")
                      : approved
                        ? "已完成"
                        : cancelled || rejected
                          ? "已结束"
                          : "状态待确认",
                  state: blocked
                      ? "blocked"
                      : running
                        ? "active"
                        : approved
                          ? "done"
                          : "muted",
                  icon:
                      blocked || rejected
                          ? CircleAlertIcon
                          : approved
                            ? CheckIcon
                            : Clock3Icon,
                  current: active,
              },
          ]
    const steps: ApprovalStep[] = [
        {
            key: "submitted",
            label: "发起审批",
            detail: "已提交",
            state: "done",
            icon: CheckIcon,
        },
        ...processingSteps,
        {
            key: "result",
            label: cancelled ? "审批撤回" : rejected ? "审批驳回" : "审批完成",
            detail: approved
                ? "已通过"
                : cancelled
                  ? "已撤回"
                  : rejected
                    ? "已驳回"
                    : "待完成",
            state: approved
                ? "approved"
                : cancelled
                  ? "cancelled"
                  : rejected
                    ? "rejected"
                    : "muted",
            icon: approved ? CheckIcon : rejected ? CircleAlertIcon : MinusIcon,
        },
    ]

    return (
        <section
            aria-label="审批进度"
            className={cn(
                "flex flex-col",
                compact
                    ? "gap-3 @min-[640px]/document:flex-row @min-[640px]/document:flex-wrap @min-[640px]/document:items-start @min-[640px]/document:gap-4"
                    : "gap-5 py-1",
            )}
        >
            <div
                className={cn(
                    "flex flex-wrap items-center justify-between gap-2",
                    compact &&
                        "@min-[640px]/document:w-28 @min-[640px]/document:shrink-0 @min-[640px]/document:flex-col @min-[640px]/document:items-start",
                )}
            >
                <h3 className="text-sm font-semibold">审批进度</h3>
                <span className="text-xs text-muted-foreground">
                    {[
                        processName && processName !== "审批流程"
                            ? processName
                            : undefined,
                        displayRound(instance.currentRoundNo),
                        version ? `v${version}` : undefined,
                    ]
                        .filter(Boolean)
                        .join(" · ")}
                </span>
            </div>
            {route ? (
                <ApprovalRoute route={route} />
            ) : (
                <ol
                    aria-label="审批阶段"
                    className={cn(
                        showVertical
                            ? "flex flex-col gap-6 pb-2"
                            : "grid grid-cols-3 pb-2",
                        compact && "min-w-0 flex-1",
                    )}
                >
                    {steps.map((step, index) => (
                        <li
                            key={step.key}
                            aria-current={step.current ? "step" : undefined}
                            className={cn(
                                "relative min-w-0",
                                showVertical
                                    ? "grid grid-cols-[2rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-left"
                                    : "flex flex-col items-center gap-2 px-2 text-center",
                            )}
                        >
                            {index < steps.length - 1 ? (
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        showVertical
                                            ? "absolute top-9 left-4 h-[calc(100%-12px)] w-px bg-border"
                                            : "absolute top-4 left-[calc(50%+22px)] h-px w-[calc(100%-44px)]",
                                        approved || (index === 0 && active)
                                            ? "bg-foreground/35"
                                            : "bg-border",
                                    )}
                                />
                            ) : null}
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "relative flex size-8 shrink-0 items-center justify-center rounded-full border",
                                    showVertical && "row-span-2",
                                    step.state === "done" &&
                                        "border-foreground/20 bg-muted text-foreground",
                                    step.state === "approved" &&
                                        "border-success-border bg-success-soft text-success-soft-foreground",
                                    step.state === "active" &&
                                        "border-primary bg-primary text-primary-foreground ring-4 ring-primary/5",
                                    (step.state === "blocked" ||
                                        step.state === "rejected") &&
                                        "border-destructive/30 bg-destructive/10 text-destructive ring-4 ring-destructive/5",
                                    (step.state === "muted" ||
                                        step.state === "cancelled") &&
                                        "border-border bg-background text-muted-foreground",
                                )}
                            >
                                <step.icon className="size-4" />
                            </span>
                            <span
                                className={cn(
                                    "max-w-full text-sm leading-5 wrap-anywhere",
                                    step.state === "active" ||
                                        step.state === "blocked"
                                        ? "font-semibold"
                                        : "font-medium",
                                    step.state === "muted" &&
                                        "text-muted-foreground",
                                )}
                            >
                                {step.round ? (
                                    <span className="block text-xs font-normal text-muted-foreground">
                                        {step.round}
                                    </span>
                                ) : null}
                                {step.label}
                            </span>
                            <span
                                className={cn(
                                    "text-xs leading-5 wrap-anywhere",
                                    showVertical && "col-start-2",
                                    step.state === "approved"
                                        ? "text-success-soft-foreground"
                                        : step.state === "blocked" ||
                                            step.state === "rejected"
                                          ? "text-destructive"
                                          : "text-muted-foreground",
                                )}
                            >
                                {step.detail}
                            </span>
                            {step.opinion ? (
                                <div
                                    className={cn(
                                        "mt-1 w-full rounded-lg border px-3 py-2 text-left",
                                        showVertical && "col-start-2",
                                        step.state === "approved"
                                            ? "border-success-border bg-success-soft text-success-soft-foreground"
                                            : step.state === "rejected"
                                              ? "border-destructive-border bg-destructive-soft text-destructive-soft-foreground"
                                              : "border-border bg-muted text-foreground",
                                    )}
                                >
                                    <WorkspaceDescriptionText
                                        text={step.opinion}
                                        splitClauses={false}
                                    />
                                </div>
                            ) : null}
                        </li>
                    ))}
                </ol>
            )}
            {route && history.length > 0 ? (
                <details className="basis-full">
                    <summary
                        id={`workspace-approval-progress-${toAutomationIdSegment(instance.id)}-history`}
                        className="cursor-pointer rounded-sm text-xs text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                        查看审批处理记录
                    </summary>
                    <div className="mt-3">
                        <ExecutionHistory items={history} compact />
                    </div>
                </details>
            ) : null}
            {historyQuery.isPending ? (
                <p className="text-xs text-muted-foreground" role="status">
                    正在加载审批进度…
                </p>
            ) : null}
            <ApprovalQueryFeedback
                surface="workspace-approval-progress"
                instanceId={instance.id}
                historyQuery={historyQuery}
            />
            {historyQuery.hasNextPage ? (
                <LoadingButton
                    id={`workspace-approval-progress-${toAutomationIdSegment(instance.id)}-load-more`}
                    variant="outline"
                    size="sm"
                    loading={historyQuery.isFetchingNextPage}
                    onClick={() => void historyQuery.fetchNextPage()}
                >
                    加载更早的审批记录
                </LoadingButton>
            ) : null}
            {blocked ? (
                <Alert
                    variant="destructive"
                    className={compact ? "basis-full" : undefined}
                >
                    <AlertTitle>审批受阻</AlertTitle>
                    <AlertDescription>
                        <WorkspaceDescriptionText
                            text={
                                instance.blockerMessage ??
                                "当前审批无法继续，请按系统给出的恢复方式处理。"
                            }
                        />
                    </AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}
