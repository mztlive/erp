"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"
import type {
    InventoryAvailability,
    InventoryListView,
    InventoryView,
} from "@/features/inventory/types"

export type LedgerMetricPatch = {
    view: InventoryView
    availability: InventoryAvailability | null
}

export type LedgerMetricActive = "combos" | "reserved" | "zero" | "pending"

interface LedgerMetricStripProps {
    metrics: InventoryListView["metrics"]
    metricActive: LedgerMetricActive
    view: InventoryView
    onSelect: (patch: LedgerMetricPatch) => void
}

export function LedgerMetricStrip({
    metrics,
    metricActive,
    view,
    onSelect,
}: LedgerMetricStripProps) {
    return (
        <MetricStrip columns={4} aria-label="库存台账指标筛选">
            {/* 指标 = view + availability 组合语义（视图快捷组合，有业务价值）：点击同时写
            view 与 availability 两个参数；与工具栏「可用状态」下拉共享 availability 参数
            天然同步；Tabs（view）与指标条同源 URL，保持一致。 */}
            <MetricFilterItem
                id="inventory-ledger-metric-combos"
                label="库存组合"
                value={metrics.balanceDimensionCount}
                detail="仓库+SKU 组合数"
                active={metricActive === "combos" && view === "balance"}
                onClick={() =>
                    onSelect({ view: "balance", availability: "all" })
                }
            />
            <MetricFilterItem
                id="inventory-ledger-metric-reserved"
                label="有效预占组合"
                value={metrics.reservedDimensionCount}
                detail="有有效预占"
                active={metricActive === "reserved"}
                onClick={() =>
                    onSelect({ view: "balance", availability: "reserved" })
                }
            />
            <MetricFilterItem
                id="inventory-ledger-metric-zero"
                label="零可用组合"
                value={metrics.zeroAvailableDimensionCount}
                detail="可用数量为 0"
                active={metricActive === "zero"}
                onClick={() =>
                    onSelect({ view: "balance", availability: "zero" })
                }
            />
            <MetricFilterItem
                id="inventory-ledger-metric-pending"
                label="待处理调整"
                value={metrics.pendingAdjustmentCount}
                detail="处理中"
                active={metricActive === "pending"}
                onClick={() =>
                    onSelect({ view: "adjustment", availability: null })
                }
            />
        </MetricStrip>
    )
}
