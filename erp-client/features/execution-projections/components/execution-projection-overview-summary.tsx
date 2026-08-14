"use client"

import { DocumentSummary } from "@/components/business"
import type { ExecutionProjectionView } from "@/features/execution-projections/types"
import {
    LATENCY_LABEL,
    SOURCE_LABEL,
} from "@/features/execution-projections/types"
import { versionText } from "@/lib/ui-text"

export function ExecutionProjectionOverviewSummary({
    detail,
}: {
    detail: ExecutionProjectionView
}) {
    return (
        <DocumentSummary
            columns="two"
            items={[
                {
                    id: "source-ver",
                    label: "来源销售版本",
                    value: `v${detail.selectedRevision.salesOrderRevisionNo}`,
                    numeric: true,
                },
                {
                    id: "proj-ver",
                    label: versionText.dataVersion,
                    value: `v${detail.selectedRevision.revisionNo}`,
                    numeric: true,
                },
                {
                    id: "source",
                    label: "数据来源",
                    value: SOURCE_LABEL[
                        detail.selectedRevision.projectionSource
                    ],
                },
                {
                    id: "acked",
                    label: "商城已确认版",
                    value:
                        detail.currentAckedRevisionNo != null
                            ? `v${detail.currentAckedRevisionNo}`
                            : "尚未确认",
                    numeric: true,
                },
                {
                    id: "latency",
                    label: "等待时长",
                    value: `${detail.pendingDurationLabel} · ${LATENCY_LABEL[detail.latencyBand]}`,
                },
                {
                    id: "owner",
                    label: "责任",
                    value: detail.ownerLabel,
                },
            ]}
        />
    )
}
