"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"
import type {
    MallConsumptionOrderMetric,
    MallConsumptionOrderMetricKey,
} from "@/features/mall-consumption-orders/types"

const METRIC_ITEMS: ReadonlyArray<{
    key: MallConsumptionOrderMetricKey
    label: string
}> = [
    { key: "paid", label: "支付成功" },
    { key: "pending_attr", label: "待归集" },
    { key: "fact_diff", label: "记录差异" },
    { key: "auto_exception", label: "自动履约异常" },
    { key: "cost_none", label: "成本未覆盖" },
]

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
