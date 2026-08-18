"use client"

import { DataFreshness, MetricItem, MetricStrip } from "@/components/business"
import { Button } from "@/components/ui/button"
import { freshnessText } from "@/lib/ui-text"
import { formatOccurredAt } from "@/features/sales-orders/lib/acceptance-model"
import type { CustomerAcceptanceWorkspaceView } from "@/features/sales-orders/lib/acceptance-types"

export function AcceptanceSummaryBar({
    metrics,
    fulfillmentProgress,
    freshness,
    remainingOnly,
    onRemainingOnlyChange,
}: {
    metrics: CustomerAcceptanceWorkspaceView["metrics"]
    fulfillmentProgress: string
    freshness: CustomerAcceptanceWorkspaceView["freshness"]
    remainingOnly: boolean
    onRemainingOnlyChange: (next: boolean) => void
}) {
    const eligibleLabel =
        metrics.eligibleQuantityByUnit.length > 0
            ? metrics.eligibleQuantityByUnit
                  .map((u) => `${u.quantity} ${u.unitCode}`)
                  .join(" · ")
            : "0"

    return (
        <>
            <div className="flex flex-wrap items-center justify-between gap-2">
                <MetricStrip columns={4} aria-label="待验收摘要">
                    <MetricItem
                        className="border-0 bg-muted/40"
                        label="待验收批次"
                        value={String(metrics.eligibleFulfillmentCount)}
                        detail="还可验收的交付记录"
                    />
                    <MetricItem
                        className="border-0 bg-muted/40"
                        label="待验收数量"
                        value={eligibleLabel}
                        detail="按单位分别统计"
                    />
                    <MetricItem
                        className="border-0 bg-muted/40"
                        label="交付进度"
                        value={fulfillmentProgress}
                        detail="本单交付情况"
                    />
                    <MetricItem
                        className="border-0 bg-muted/40"
                        label={freshnessText.dataUpdatedAt}
                        value={
                            <DataFreshness
                                updatedAt={formatOccurredAt(
                                    freshness.factsUpdatedAt,
                                )}
                                dateTime={freshness.factsUpdatedAt}
                                state={freshness.state}
                                label="交付/验收记录"
                            />
                        }
                    />
                </MetricStrip>
            </div>

            <div className="flex flex-wrap items-center gap-2 text-sm">
                <span className="text-muted-foreground">交付记录筛选</span>
                <Button
                    type="button"
                    size="sm"
                    variant={remainingOnly ? "secondary" : "ghost"}
                    onClick={() => onRemainingOnlyChange(true)}
                >
                    仅待验收
                </Button>
                <Button
                    type="button"
                    size="sm"
                    variant={!remainingOnly ? "secondary" : "ghost"}
                    onClick={() => onRemainingOnlyChange(false)}
                >
                    全部历史记录
                </Button>
            </div>
        </>
    )
}
