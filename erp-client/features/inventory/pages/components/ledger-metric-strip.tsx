"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import type { InventoryListView } from "@/features/inventory/types"

export function LedgerMetricStrip({
    metrics,
}: {
    metrics: InventoryListView["metrics"]
}) {
    return (
        <MetricStrip className="mb-6" columns={4} aria-label="库存台账指标">
            <MetricItem
                id="inventory-ledger-metric-combos"
                label="库存组合"
                value={metrics.balanceDimensionCount}
                detail="仓库+SKU 组合数"
            />
            <MetricItem
                id="inventory-ledger-metric-reserved"
                label="有效预占组合"
                value={metrics.reservedDimensionCount}
                detail="有有效预占"
            />
            <MetricItem
                id="inventory-ledger-metric-zero"
                label="零可用组合"
                value={metrics.zeroAvailableDimensionCount}
                detail="可用数量为 0"
            />
            <MetricItem
                id="inventory-ledger-metric-pending"
                label="待处理调整"
                value={metrics.pendingAdjustmentCount}
                detail="处理中"
            />
        </MetricStrip>
    )
}
