"use client"

import * as React from "react"

import type { RevisionTimelineEntry } from "@/components/business"
import { Badge } from "@/components/ui/badge"
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
                <div className="flex flex-wrap gap-2">
                    {options.showTiming && rev.timingLabel ? (
                        <Badge variant="outline">{rev.timingLabel}</Badge>
                    ) : null}
                    <Badge variant="secondary">
                        {rev.lifecycleAtRevision === "ENABLED"
                            ? "启用"
                            : "停用"}
                    </Badge>
                </div>
                {options.extra}
            </div>
        ),
        isCurrent: rev.isCurrent,
    }
}
