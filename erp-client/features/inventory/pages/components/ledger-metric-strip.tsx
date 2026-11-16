"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import type { InventoryListView } from "@/features/inventory/types"

export function LedgerMetricStrip({
    metrics,
}: {
    metrics: InventoryListView["metrics"]
}) {
    return (
        <MetricStrip className="mb-4" columns={4} aria-label="库存台账指标">
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
            <MetricItem
                id="inventory-ledger-metric-pending"
                label="待处理调整"
                value={metrics.pendingAdjustmentCount}
                className="border-l border-border/70 pl-4 lg:pl-6"
            />
        </MetricStrip>
    )
}
