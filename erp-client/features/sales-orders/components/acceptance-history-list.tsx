"use client"

import { RotateCcwIcon } from "lucide-react"

import { BusinessStatusBadge, DocumentSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import { formatOccurredAt } from "@/features/sales-orders/lib/acceptance-model"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    FACT_ONLY_NOTICE,
    OVERALL_RESULT_LABEL,
    type AcceptanceHistoryItem,
} from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceHistoryList({
    history,
    canReverse,
    onReverse,
    className,
}: {
    history: AcceptanceHistoryItem[]
    canReverse: boolean
    onReverse: (item: AcceptanceHistoryItem) => void
    className?: string
}) {
    return (
        <DocumentSection
            className={className ?? "py-0"}
            title="验收记录"
            description="已经确认的不能改；记错了用冲正新增一条反向记录。"
        >
            {history.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    还没有验收记录。
                </p>
            ) : (
                <ol className="space-y-3">
                    {history.map((item) => (
                        <li
                            key={item.acceptanceId}
                            className="rounded-lg border border-border px-3 py-2 text-sm"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div>
                                    <span className="num font-mono font-medium">
                                        {item.acceptanceNo}
                                    </span>
                                    <BusinessStatusBadge
                                        context="preview"
                                        label={
                                            item.reversalOfAcceptanceId
                                                ? "冲正记录"
                                                : item.status === "REVERSED"
                                                  ? "已冲正"
                                                  : "已确认"
                                        }
                                        tone={
                                            item.status === "REVERSED"
                                                ? "void"
                                                : item.reversalOfAcceptanceId
                                                  ? "warning"
                                                  : "success"
                                        }
                                        className="ms-2"
                                    />
                                </div>
                                <span className="text-xs text-muted-foreground">
                                    {OVERALL_RESULT_LABEL[item.overallResult]}
                                    {item.postedAt
                                        ? ` · ${formatOccurredAt(item.postedAt)}`
                                        : ""}
                                </span>
                            </div>
                            {(item.overallResult === "SHORT" ||
                                item.overallResult === "REJECT" ||
                                item.overallResult === "SERVICE_FAIL") &&
                            !item.reversalOfAcceptanceId ? (
                                <p className="mt-1 text-xs text-warning-soft-foreground">
                                    {FACT_ONLY_NOTICE}
                                </p>
                            ) : null}
                            {item.status === "POSTED" &&
                            !item.reversalOfAcceptanceId &&
                            canReverse ? (
                                <Button
                                    id={`sales-orders-acceptance-history-${toAutomationIdSegment(item.acceptanceId)}-reverse`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="mt-1"
                                    onClick={() => onReverse(item)}
                                >
                                    <RotateCcwIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    冲正误录
                                </Button>
                            ) : null}
                        </li>
                    ))}
                </ol>
            )}
        </DocumentSection>
    )
}
