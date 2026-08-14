"use client"

import { RotateCcwIcon } from "lucide-react"

import { BusinessStatusBadge, DocumentSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    OVERALL_RESULT_LABEL,
    type AcceptanceHistoryItem,
} from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceHistoryList({
    history,
    canReverse,
    onReverse,
}: {
    history: AcceptanceHistoryItem[]
    canReverse: boolean
    onReverse: (item: AcceptanceHistoryItem) => void
}) {
    return (
        <DocumentSection
            className="py-0"
            title="验收历史"
            description="已确认不可编辑；误录通过新的反向记录分配纠正。"
        >
            <div className="space-y-3">
                {history.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        暂无历史记录。
                    </p>
                ) : (
                    <ul className="space-y-3" role="list">
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
                                    </span>
                                </div>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    {item.lines
                                        .map(
                                            (l) =>
                                                `通过 ${l.acceptedQuantity} / 短少 ${l.shortQuantity} / 拒收 ${l.rejectedQuantity}`,
                                        )
                                        .join("；")}
                                </p>
                                {(item.overallResult === "SHORT" ||
                                    item.overallResult === "REJECT" ||
                                    item.overallResult === "SERVICE_FAIL") &&
                                !item.reversalOfAcceptanceId ? (
                                    <p className="mt-1 text-xs text-warning-soft-foreground">
                                        {item.factOnlyNotice}
                                    </p>
                                ) : null}
                                {item.status === "POSTED" &&
                                !item.reversalOfAcceptanceId &&
                                canReverse ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        className="mt-2"
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
                    </ul>
                )}
            </div>
        </DocumentSection>
    )
}
