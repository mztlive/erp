"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"
import type {
    ExecutionProjectionMetric,
    ExecutionProjectionMetricKey,
} from "@/features/execution-projections/types"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

export function ExecutionProjectionMetricStrip({
    metrics,
    metric,
    replaceParams,
}: {
    metrics: ExecutionProjectionMetric[]
    metric: ExecutionProjectionMetricKey | "all"
    replaceParams: ReplaceParams
}) {
    return (
        <MetricStrip columns={5} aria-label="执行信息指标筛选">
            {metrics.map((m) => (
                <MetricFilterItem
                    key={m.key}
                    label={m.label}
                    value={m.value}
                    detail={m.detail}
                    active={metric === m.key}
                    onClick={() =>
                        replaceParams({
                            metric: metric === m.key ? null : m.key,
                            page: "1",
                        })
                    }
                />
            ))}
        </MetricStrip>
    )
}
