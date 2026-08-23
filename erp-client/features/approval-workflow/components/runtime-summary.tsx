"use client"

import type * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { cn } from "@/lib/utils"

import {
    displayInstanceStatus,
    displayProcessVersion,
    displayRound,
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
            <Card size="sm" className={className}>
                <CardHeader>
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

    return (
        <Card
            size="sm"
            className={cn(
                className,
                blocked && "border-destructive/40 bg-destructive/5",
            )}
            data-blocked={blocked ? "true" : "false"}
        >
            <CardHeader>
                <SummaryHeading compact={compact}>{processLabel}</SummaryHeading>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
                <p>审批状态：{displayInstanceStatus(instance.status)}</p>
                <p>当前轮次：{displayRound(instance.currentRoundNo)}</p>
                <p>
                    当前节点：
                    {instance.currentNodeName ?? instance.currentNode ?? "—"}
                </p>
                <p>
                    当前审批人：
                    {instance.currentAssigneeName ??
                        instance.currentAssignee ??
                        "—"}
                </p>
                {instance.latestRejection ? (
                    <p>
                        最近驳回：
                        {instance.latestRejectionBy
                            ? `${instance.latestRejectionBy} / `
                            : ""}
                        {instance.latestRejection}
                    </p>
                ) : (
                    <p>最近驳回：无</p>
                )}
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
