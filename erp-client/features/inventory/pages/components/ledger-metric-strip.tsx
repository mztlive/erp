"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import type { InventoryListView } from "@/features/inventory/types"

export function LedgerMetricStrip({
    metrics,
    showBalanceMetrics,
    showAdjustmentMetrics,
}: {
    metrics: InventoryListView["metrics"]
    showBalanceMetrics: boolean
    showAdjustmentMetrics: boolean
}) {
    if (!showBalanceMetrics && !showAdjustmentMetrics) return null
    return (
        <MetricStrip
            variant="plain"
            columns={showBalanceMetrics ? (showAdjustmentMetrics ? 4 : 3) : 1}
            className="grid-cols-2 gap-y-2 [&_[data-slot=metric-item]_.num]:text-body-compact"
            role="group"
            aria-label="库存台账指标"
        >
            {showBalanceMetrics ? (
                <>
                    <MetricItem
                        id="inventory-ledger-metric-combos"
                        label="在库品项数"
                        value={metrics.balanceDimensionCount}
                        detail="按仓库与 SKU 组合统计"
                        detailMode="tooltip"
                    />
                    <MetricItem
                        id="inventory-ledger-metric-reserved"
                        label="有预留品项"
                        value={metrics.reservedDimensionCount}
                    />
                    <MetricItem
                        id="inventory-ledger-metric-zero"
                        label="缺货/零可用品项"
                        value={metrics.zeroAvailableDimensionCount}
                        detail="可用数量为 0"
                        detailMode="tooltip"
                    />
                </>
            ) : null}
            {showAdjustmentMetrics ? (
                <MetricItem
                    id="inventory-ledger-metric-pending"
                    label="待处理调整"
                    value={metrics.pendingAdjustmentCount}
                />
            ) : null}
        </MetricStrip>
    )
}
