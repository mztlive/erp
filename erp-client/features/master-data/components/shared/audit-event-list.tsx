"use client"

import { StatusBadge } from "@/components/ui/status-badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"

/** 主数据对象中心共用的审计记录列表。 */
export function MasterDataAuditList({
    events,
}: {
    events: MasterDataCenterView["auditEvents"]
}) {
    if (events.length === 0) {
        return (
            <p className="text-xs text-muted-foreground">
                {masterDataCopy.centerNoAudit}
            </p>
        )
    }
    return (
        <ul className="divide-y divide-border/60 text-body-compact">
            {events.map((ev) => (
                <li key={ev.id} className="py-2.5 first:pt-0 last:pb-0">
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="num text-xs text-muted-foreground">
                            {formatDateTime(ev.at, "full", "passthrough")}
                        </span>
                        <span className="font-medium text-foreground">
                            {ev.actor}
                        </span>
                        <StatusBadge
                            tone="neutral"
                            appearance="quiet"
                            label={ev.action}
                        />
                    </div>
                    <div className="mt-1 text-muted-foreground">
                        {ev.detail}
                    </div>
                </li>
            ))}
        </ul>
    )
}
