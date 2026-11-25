"use client"

import { DecisionKpiBar, DecisionKpiCell } from "@/components/business"
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
    const columns = showBalanceMetrics ? (showAdjustmentMetrics ? 4 : 3) : 1
    return (
        <DecisionKpiBar
            columns={columns}
            ariaLabel="库存台账指标"
            gridClassName="py-2.5"
        >
            {showBalanceMetrics ? (
                <>
                    <DecisionKpiCell
                        id="inventory-ledger-metric-combos"
                        label="在库品项数"
                        emphasis="terms"
                        value={metrics.balanceDimensionCount}
                        formulaTooltip="按仓库与 SKU 组合统计"
                        formulaTooltipId="inventory-ledger-metric-combos-formula"
                    />
                    <DecisionKpiCell
                        id="inventory-ledger-metric-reserved"
                        label="有预留品项"
                        emphasis="terms"
                        value={metrics.reservedDimensionCount}
                    />
                    <DecisionKpiCell
                        id="inventory-ledger-metric-zero"
                        label="缺货/零可用品项"
                        emphasis="terms"
                        value={metrics.zeroAvailableDimensionCount}
                        formulaTooltip="可用数量为 0"
                        formulaTooltipId="inventory-ledger-metric-zero-formula"
                        fillThreeColMidRow={columns === 3}
                    />
                </>
            ) : null}
            {showAdjustmentMetrics ? (
                <DecisionKpiCell
                    id="inventory-ledger-metric-pending"
                    label="待处理调整"
                    emphasis="terms"
                    value={metrics.pendingAdjustmentCount}
                />
            ) : null}
        </DecisionKpiBar>
    )
}
