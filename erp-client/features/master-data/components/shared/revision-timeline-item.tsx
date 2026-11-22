"use client"

import * as React from "react"

import type { RevisionTimelineEntry } from "@/components/business"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export type MasterDataRevisionSnapshot = {
    id: string
    revisionNo: number
    actor: string
    effectiveFrom: string
    nameSnapshot: string
    changeReason: string
    lifecycleAtRevision: "ENABLED" | "DISABLED"
    isCurrent: boolean
    timingLabel?: string
}

function timingTone(timingLabel: string): StatusTone {
    if (timingLabel === masterDataCopy.timingFuture) return "warning"
    if (timingLabel === masterDataCopy.timingCurrent) return "info"
    return "neutral"
}

/**
 * 主数据版本时间线的一条记录。
 * 生效文案、是否显示版本时机、变更原因是否弱化，以及额外明细由调用方决定。
 */
export function masterDataRevisionEntry(
    rev: MasterDataRevisionSnapshot,
    options: {
        effectiveLabel: string
        showTiming?: boolean
        muteChangeReason?: boolean
        extra?: React.ReactNode
    },
): RevisionTimelineEntry {
    return {
        id: rev.id,
        version: rev.revisionNo,
        source: "erp-change",
        actor: rev.actor,
        effectiveAt: {
            dateTime: rev.effectiveFrom,
            label: options.effectiveLabel,
        },
        reason: (
            <div className="space-y-1">
                <div>
                    {masterDataCopy.centerHistoryName}：
                    <strong>{rev.nameSnapshot}</strong>
                </div>
                <div
                    className={
                        options.muteChangeReason
                            ? "text-muted-foreground"
                            : undefined
                    }
                >
                    {rev.changeReason}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    {options.showTiming && rev.timingLabel ? (
                        <StatusBadge
                            tone={timingTone(rev.timingLabel)}
                            appearance="quiet"
                            label={rev.timingLabel}
                        />
                    ) : null}
                    <StatusBadge
                        tone={
                            rev.lifecycleAtRevision === "ENABLED"
                                ? "success"
                                : "neutral"
                        }
                        appearance="quiet"
                        label={
                            rev.lifecycleAtRevision === "ENABLED"
                                ? "启用"
                                : "停用"
                        }
                    />
                </div>
                {options.extra}
            </div>
        ),
        isCurrent: rev.isCurrent,
    }
}
