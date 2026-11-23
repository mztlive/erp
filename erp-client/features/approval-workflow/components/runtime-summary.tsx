"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

import {
    displayActorName,
    displayInstanceStatus,
    displayProcessVersion,
    displayReadableName,
    displayRound,
    instanceStatusTone,
    isBlockedStatus,
} from "../display"
import type { ApprovalRuntimeInstance } from "../types"

/**
 * 运行中或终态单据的审批摘要。
 *
 * `BLOCKED` 使用独立受阻样式，不得伪装为普通待办。
 */
function SummaryFact({
    label,
    value,
    emphasized,
}: {
    label: string
    value: string
    emphasized?: boolean
}) {
    return (
        <DescriptionItem>
            <DescriptionTerm>{label}</DescriptionTerm>
            <DescriptionDetails className={cn(emphasized && "font-medium")}>
                {value}
            </DescriptionDetails>
        </DescriptionItem>
    )
}

export function RuntimeSummary({
    instance,
    className,
    compact = false,
    variant = "card",
}: {
    instance?: ApprovalRuntimeInstance
    className?: string
    /** 对象中心 tab 或侧边抽屉内使用：标题与正文紧凑字号对齐。 */
    compact?: boolean
    /** 兼容旧调用参数；当前统一渲染无阴影扁平表面。 */
    variant?: "card" | "flat"
}) {
    void variant

    const headingClassName = cn(
        "font-semibold tracking-tight text-foreground",
        compact ? "text-body-compact" : "text-section-title",
    )

    if (!instance) {
        return (
            <section
                data-slot="approval-surface"
                data-blocked="false"
                className={cn("space-y-2", className)}
            >
                <h2 className={headingClassName}>审批摘要</h2>
                <p className="text-body-compact text-muted-foreground">
                    当前没有可展示的审批进度
                </p>
            </section>
        )
    }

    const blocked = isBlockedStatus(instance.status)
    const processLabel = displayProcessVersion({
        name: instance.processName,
        version: instance.processVersion,
    })
    const currentNode =
        displayReadableName(instance.currentNodeName, instance.currentNode) ??
        "审批节点未标注"
    const currentAssignee =
        displayActorName(
            instance.currentAssigneeName,
            instance.currentAssignee,
        ) ?? "审批人待确认"
    const rejectionBy = displayActorName(
        instance.latestRejectionByName,
        instance.latestRejectionBy,
    )

    return (
        <section
            data-slot="approval-surface"
            data-blocked={blocked ? "true" : "false"}
            className={cn(
                "space-y-4",
                blocked &&
                    "rounded-lg border border-destructive-border bg-destructive-soft/40 p-4",
                className,
            )}
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h2 className={headingClassName}>{processLabel}</h2>
                <StatusBadge
                    tone={instanceStatusTone(instance.status)}
                    label={displayInstanceStatus(instance.status)}
                />
            </div>
            <DescriptionList columns="three">
                <SummaryFact
                    label="当前审批人"
                    value={currentAssignee}
                    emphasized
                />
                <SummaryFact label="当前节点" value={currentNode} />
                <SummaryFact
                    label="当前轮次"
                    value={displayRound(instance.currentRoundNo)}
                />
            </DescriptionList>
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
            {blocked ? (
                <Alert variant="destructive">
                    <AlertTitle>审批受阻</AlertTitle>
                    <AlertDescription>
                        {instance.blockerMessage ??
                            "当前审批无法继续，请按系统给出的恢复方式处理。"}
                    </AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}
