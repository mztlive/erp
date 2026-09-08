"use client"

import { CheckIcon, CircleAlertIcon, Clock3Icon, MinusIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    displayActorName,
    displayRound,
} from "@/features/approval-workflow/display"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import { cn } from "@/lib/utils"

/** 展示提交、当前办理与结果三个阶段，不把阶段位置当作审批节点完成比例。 */
export function WorkspaceApprovalProgress({
    instance,
}: {
    instance: ApprovalRuntimeInstance
}) {
    const approved = instance.status === "APPROVED"
    const cancelled = instance.status === "CANCELLED"
    const blocked = instance.status === "BLOCKED"
    const running = instance.status === "RUNNING"
    const active = running || blocked
    const assignee =
        displayActorName(instance.currentAssigneeName) ??
        displayActorName(instance.currentAssignee)
    const version =
        instance.processVersion == null
            ? ""
            : String(instance.processVersion).trim()
    const processName = instance.processName?.trim()
    const rejectionBy = displayActorName(instance.latestRejectionBy)
    const steps = [
        { label: "发起审批", detail: "已提交", state: "done", icon: CheckIcon },
        {
            label: active
                ? instance.currentNodeName?.trim() || "审批处理"
                : "审批处理",
            detail: active
                ? [
                      assignee || "审批人待确认",
                      blocked ? "受阻" : "审批中",
                  ].join(" · ")
                : approved
                  ? "已完成"
                  : cancelled
                    ? "已结束"
                    : "状态待确认",
            state: blocked
                ? "blocked"
                : running
                  ? "active"
                  : approved
                    ? "done"
                    : "muted",
            icon: blocked ? CircleAlertIcon : approved ? CheckIcon : Clock3Icon,
        },
        {
            label: cancelled ? "审批撤回" : "审批完成",
            detail: approved ? "已通过" : cancelled ? "已撤回" : "待完成",
            state: approved ? "done" : cancelled ? "cancelled" : "muted",
            icon: approved ? CheckIcon : MinusIcon,
        },
    ]

    return (
        <section aria-label="审批进度" className="flex flex-col gap-5 py-1">
            <div className="flex flex-wrap items-center justify-between gap-2">
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
            <ol aria-label="审批阶段" className="grid grid-cols-3 pb-2">
                {steps.map((step, index) => (
                    <li
                        key={["submitted", "processing", "result"][index]}
                        aria-current={
                            active && index === 1 ? "step" : undefined
                        }
                        className="relative flex min-w-0 flex-col items-center gap-2 px-2 text-center"
                    >
                        {index < steps.length - 1 ? (
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "absolute top-4 left-[calc(50%+22px)] h-px w-[calc(100%-44px)]",
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
                                step.state === "done" &&
                                    "border-foreground/20 bg-muted text-foreground",
                                step.state === "active" &&
                                    "border-foreground bg-foreground text-background ring-4 ring-foreground/5",
                                step.state === "blocked" &&
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
                            {step.label}
                        </span>
                        <span
                            className={cn(
                                "text-xs leading-5 wrap-anywhere",
                                step.state === "blocked"
                                    ? "text-destructive"
                                    : "text-muted-foreground",
                            )}
                        >
                            {step.detail}
                        </span>
                    </li>
                ))}
            </ol>
            {blocked ? (
                <Alert variant="destructive">
                    <AlertTitle>审批受阻</AlertTitle>
                    <AlertDescription>
                        {instance.blockerMessage ??
                            "当前审批无法继续，请按系统给出的恢复方式处理。"}
                    </AlertDescription>
                </Alert>
            ) : null}
            {instance.latestRejection ? (
                <Alert variant="destructive">
                    <AlertTitle>
                        最近驳回{rejectionBy ? ` · ${rejectionBy}` : ""}
                    </AlertTitle>
                    <AlertDescription>
                        {instance.latestRejection}
                    </AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}
