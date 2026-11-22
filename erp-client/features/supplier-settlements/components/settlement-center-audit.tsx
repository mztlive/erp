"use client"

import { DetailWorkspaceSection } from "@/components/business/detail-workspace"
import { AUDIT_ACTION_LABEL } from "@/features/supplier-settlements/types"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"

function SettlementCenterAudit({
    events,
}: {
    events: SettlementDetailView["auditEvents"]
}) {
    return (
        <DetailWorkspaceSection title="审计" width="form">
            {events.map((e) => (
                <div
                    key={e.eventId}
                    className="border-t border-border/60 py-2 text-body-compact"
                >
                    <div className="flex flex-wrap gap-2">
                        <span className="font-medium">
                            {AUDIT_ACTION_LABEL[e.action] ??
                                e.summary.split("·")[0]}
                        </span>
                        <span className="text-muted-foreground">{e.actor}</span>
                        {e.auditNo ? (
                            <span className="num text-xs">
                                审计号 {e.auditNo}
                            </span>
                        ) : null}
                    </div>
                    <p className="text-muted-foreground">{e.summary}</p>
                    <p className="text-xs text-muted-foreground">
                        {formatDateTime(e.at, "default")}
                    </p>
                </div>
            ))}
        </DetailWorkspaceSection>
    )
}

export { SettlementCenterAudit }
