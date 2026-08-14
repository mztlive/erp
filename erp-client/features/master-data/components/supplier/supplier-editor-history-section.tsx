"use client"

import { DocumentSection, RevisionTimeline } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"

export function SupplierEditorHistorySection({
    data,
}: {
    data: MasterDataCenterView
}) {
    return (
        <div className="-mt-5">
            <DocumentSection
                title={masterDataCopy.centerVersions}
                description={masterDataCopy.centerVersionsDesc}
            >
                <RevisionTimeline
                    revisions={data.revisionTimeline.map((rev) => ({
                        id: rev.id,
                        version: rev.revisionNo,
                        source: "erp-change" as const,
                        actor: rev.actor,
                        effectiveAt: {
                            dateTime: rev.effectiveFrom,
                            label: `创建于 ${rev.effectiveFrom}`,
                        },
                        reason: (
                            <div className="space-y-1">
                                <div>
                                    {masterDataCopy.centerHistoryName}：
                                    <strong>{rev.nameSnapshot}</strong>
                                </div>
                                <div>{rev.changeReason}</div>
                                <div className="flex flex-wrap gap-2">
                                    <Badge variant="secondary">
                                        {rev.lifecycleAtRevision === "ENABLED"
                                            ? "启用"
                                            : "停用"}
                                    </Badge>
                                </div>
                            </div>
                        ),
                        isCurrent: rev.isCurrent,
                    }))}
                />
            </DocumentSection>

            <DocumentSection
                title={masterDataCopy.centerRelations}
                description={masterDataCopy.centerRelationsDesc}
            >
                <p className="text-sm">
                    {masterDataCopy.centerUsageCount(
                        data.usageSummary.historicalReferenceCount,
                    )}
                    {data.usageSummary.note}
                </p>
                <ul className="mt-3 space-y-2">
                    {data.selectorEligibility.map((s) => (
                        <li
                            key={s.context}
                            className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                        >
                            <span>{s.contextLabel}</span>
                            <Badge
                                variant={
                                    s.eligible ? "success" : "destructive"
                                }
                            >
                                {s.eligible
                                    ? masterDataCopy.eligible
                                    : masterDataCopy.ineligible}
                            </Badge>
                            {s.reason ? (
                                <span className="text-xs text-muted-foreground">
                                    {s.reason}
                                </span>
                            ) : null}
                        </li>
                    ))}
                </ul>
            </DocumentSection>

            <DocumentSection
                title={masterDataCopy.centerAudit}
                description={masterDataCopy.centerAuditDesc}
            >
                {data.auditEvents.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        {masterDataCopy.centerNoAudit}
                    </p>
                ) : (
                    <ul className="space-y-2 text-sm">
                        {data.auditEvents.map((ev) => (
                            <li
                                key={ev.id}
                                className="rounded-md border border-border px-3 py-2"
                            >
                                <div className="flex flex-wrap gap-2">
                                    <span className="num text-xs text-muted-foreground">
                                        {formatDateTime(
                                            ev.at,
                                            "full",
                                            "passthrough",
                                        )}
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
                )}
            </DocumentSection>
        </div>
    )
}
