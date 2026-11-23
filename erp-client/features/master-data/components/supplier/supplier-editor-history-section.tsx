"use client"

import { DetailWorkspaceSection, RevisionTimeline } from "@/components/business"
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
        <>
            <DetailWorkspaceSection
                title={masterDataCopy.centerVersions}
                width="form"
            >
                <RevisionTimeline
                    revisions={data.revisionTimeline.map((rev) =>
                        masterDataRevisionEntry(rev, {
                            effectiveLabel: `创建于 ${rev.effectiveFrom}`,
                        }),
                    )}
                />
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title={masterDataCopy.centerAudit}
                width="form"
            >
                <MasterDataAuditList events={data.auditEvents} />
            </DetailWorkspaceSection>
        </>
    )
}
