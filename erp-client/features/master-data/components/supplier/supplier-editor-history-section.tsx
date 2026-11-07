"use client"

import { DocumentSection, RevisionTimeline } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { MasterDataAuditList } from "@/features/master-data/components/shared/audit-event-list"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataCenterView } from "@/features/master-data/types"

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
                title={masterDataCopy.centerAudit}
                description={masterDataCopy.centerAuditDesc}
            >
                <MasterDataAuditList events={data.auditEvents} />
            </DocumentSection>
        </div>
    )
}
