"use client"

import { Badge } from "@/components/ui/badge"
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
            <p className="text-sm text-muted-foreground">
                {masterDataCopy.centerNoAudit}
            </p>
        )
    }
    return (
        <ul className="space-y-2 text-sm">
            {events.map((ev) => (
                <li
                    key={ev.id}
                    className="rounded-md border border-border px-3 py-2"
                >
                    <div className="flex flex-wrap gap-2">
                        <span className="num text-xs text-muted-foreground">
                            {formatDateTime(ev.at, "full", "passthrough")}
                        </span>
                        <span>{ev.actor}</span>
                        <Badge variant="outline">{ev.action}</Badge>
                    </div>
                    <div className="mt-1 text-muted-foreground">
                        {ev.detail}
                    </div>
                </li>
            ))}
        </ul>
    )
}
