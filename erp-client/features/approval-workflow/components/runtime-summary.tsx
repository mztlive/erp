"use client"

import type * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
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
function SummaryHeading({
    compact,
    children,
}: {
    compact?: boolean
    children: React.ReactNode
}) {
    if (compact) {
        return <h2 className="text-sm font-medium">{children}</h2>
    }
    return <CardTitle>{children}</CardTitle>
}

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
}: {
    instance?: ApprovalRuntimeInstance
    className?: string
    /** 对象中心 tab 内使用：标题与概览 text-sm 对齐，避免 CardTitle 偏大。 */
    compact?: boolean
}) {
    if (!instance) {
        return (
            <Card
                size="sm"
                className={cn("border border-border shadow-sm", className)}
            >
                <CardHeader className="border-b">
                    <SummaryHeading compact={compact}>审批摘要</SummaryHeading>
                </CardHeader>
                <CardContent className="text-sm text-muted-foreground">
                    当前没有可展示的审批进度
                </CardContent>
            </Card>
        )
    }

    const blocked = isBlockedStatus(instance.status)
    const processLabel = displayProcessVersion({
        name: instance.processName,
        version: instance.processVersion,
    })
    const currentNode = instance.currentNodeName ?? instance.currentNode ?? "—"
    const currentAssignee =
        displayActorName(instance.currentAssigneeName) ??
        displayActorName(instance.currentAssignee) ??
        "—"
    const rejectionBy = displayActorName(instance.latestRejectionBy)

    return (
        <Card
            size="sm"
            className={cn(
                "border border-border shadow-sm",
                blocked && "border-destructive-border bg-destructive/5",
                className,
            )}
            data-blocked={blocked ? "true" : "false"}
        >
            <CardHeader className="border-b">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <SummaryHeading compact={compact}>
                        {processLabel}
                    </SummaryHeading>
                    <StatusBadge
                        tone={instanceStatusTone(instance.status)}
                        label={displayInstanceStatus(instance.status)}
                    />
                </div>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
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
            </CardContent>
        </Card>
    )
}
