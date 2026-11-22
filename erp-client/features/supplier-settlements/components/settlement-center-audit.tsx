"use client"

import {
    BusinessEmptyState,
    DetailWorkspaceSection,
} from "@/components/business"
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
            {events.length === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无审计记录"
                    description="本结算单产生刷新试算、差异结论或复核操作后，将按时间顺序展示审计轨迹。"
                />
            ) : (
                events.map((e) => (
                    <div
                        key={e.eventId}
                        className="border-t border-border/60 py-2 text-body-compact first:border-t-0 first:pt-0"
                    >
                        <div className="flex flex-wrap gap-2">
                            <span className="font-medium">
                                {AUDIT_ACTION_LABEL[e.action] ??
                                    e.summary.split("·")[0]}
                            </span>
                            <span className="text-muted-foreground">
                                {e.actor}
                            </span>
                            {e.auditNo ? (
                                <span className="num text-xs">
                                    审计号 {e.auditNo}
                                </span>
                            ) : null}
                        </div>
                        <p className="text-muted-foreground">{e.summary}</p>
                        <p className="num text-xs text-muted-foreground">
                            {formatDateTime(e.at, "default")}
                        </p>
                    </div>
                ))
            )}
        </DetailWorkspaceSection>
    )
}

export { SettlementCenterAudit }
