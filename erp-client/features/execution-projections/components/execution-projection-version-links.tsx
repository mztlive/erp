"use client"

import { DocumentSection, RevisionTimeline } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ExecutionProjectionView } from "@/features/execution-projections/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

export function ExecutionProjectionVersionLinks({
    detail,
    replaceParams,
}: {
    detail: ExecutionProjectionView
    replaceParams: ReplaceParams
}) {
    return (
        <DocumentSection
            title="版本对应"
            description="历史数据固定显示来源销售版本，不被销售单当前版覆盖。"
        >
            <RevisionTimeline
                revisions={detail.revisionLinks.map((link) => ({
                    id: link.projectionRevisionId,
                    version: link.projectionRevisionNo,
                    source:
                        detail.selectedRevision.projectionSource ===
                        "MIGRATION_BASELINE"
                            ? ("migration-baseline" as const)
                            : ("erp-change" as const),
                    actor: "系统",
                    effectiveAt: {
                        dateTime: link.mallAckAt ?? "2026-08-01T00:00:00+08:00",
                        label: link.mallAckAt
                            ? `确认 ${link.mallAckAt}`
                            : "尚未确认",
                    },
                    isCurrent: link.isCurrentSelection,
                    status: {
                        label: link.deliveryStatusLabel,
                        tone:
                            link.deliveryStatus === "ACKED"
                                ? ("success" as const)
                                : link.deliveryStatus === "FAILED"
                                  ? ("destructive" as const)
                                  : ("neutral" as const),
                    },
                    reason: (
                        <span>
                            来源销售版本 v{link.sourceSalesRevisionNo}
                            {link.isCurrentSelection ? " · 当前查看" : ""}
                        </span>
                    ),
                    action: (
                        <Button
                            id={`execution-projections-detail-${toAutomationIdSegment(detail.identity.projectionId)}-revision-${toAutomationIdSegment(link.projectionRevisionId)}-view`}
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={() =>
                                replaceParams({
                                    projectionId: detail.identity.projectionId,
                                    revision: link.projectionRevisionId,
                                })
                            }
                        >
                            查看此修订
                        </Button>
                    ),
                }))}
            />
        </DocumentSection>
    )
}
