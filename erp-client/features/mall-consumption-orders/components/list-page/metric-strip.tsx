"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"
import {
    MALL_CONSUMPTION_METRIC_LABELS,
    type MallConsumptionOrderMetric,
    type MallConsumptionOrderMetricKey,
} from "@/features/mall-consumption-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

const METRIC_ITEMS: ReadonlyArray<{
    key: MallConsumptionOrderMetricKey
    label: string
}> = (
    Object.keys(
        MALL_CONSUMPTION_METRIC_LABELS,
    ) as MallConsumptionOrderMetricKey[]
).map((key) => ({ key, label: MALL_CONSUMPTION_METRIC_LABELS[key] }))

type Props = {
    metrics: MallConsumptionOrderMetric[]
    activeMetric: MallConsumptionOrderMetricKey | "all"
    periodSelected: boolean
    onToggleMetric: (key: MallConsumptionOrderMetricKey) => void
}

export function ConsumptionMetricStrip({
    metrics,
    activeMetric,
    periodSelected,
    onToggleMetric,
}: Props) {
    const valueOf = (key: MallConsumptionOrderMetricKey) =>
        metrics.find((m) => m.key === key)?.value ?? "—"

    return (
        <MetricStrip columns={5} aria-label="消费订单指标筛选">
            {METRIC_ITEMS.map((item) => (
                <MetricFilterItem
                    key={item.key}
                    id={`mall-consumption-orders-metric-${toAutomationIdSegment(item.key)}`}
                    label={item.label}
                    value={valueOf(item.key)}
                    active={activeMetric === item.key}
                    disabled={!periodSelected}
                    title={periodSelected ? undefined : "选择期间后可筛选"}
                    onClick={() => onToggleMetric(item.key)}
                />
            ))}
        </MetricStrip>
    )
}
