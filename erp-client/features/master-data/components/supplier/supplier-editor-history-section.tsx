"use client"

import { DocumentSection, RevisionTimeline } from "@/components/business"
import { MasterDataAuditList } from "@/features/master-data/components/shared/audit-event-list"
import { masterDataRevisionEntry } from "@/features/master-data/components/shared/revision-timeline-item"
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
                    revisions={data.revisionTimeline.map((rev) =>
                        masterDataRevisionEntry(rev, {
                            effectiveLabel: `创建于 ${rev.effectiveFrom}`,
                        }),
                    )}
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
