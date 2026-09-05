"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import type { ConnectionListView } from "@/features/supplier-api-connections/types"

export function ConnectionMetricStrip({
    data,
}: {
    data: ConnectionListView | undefined
}) {
    return (
        <MetricStrip columns={5} aria-label="连接指标">
            <MetricItem
                id="supplier-api-connections-metric-enabled"
                label="已启用"
                value={data?.metrics.enabled ?? 0}
            />
            <MetricItem
                id="supplier-api-connections-metric-faulted"
                label="故障"
                value={data?.metrics.faulted ?? 0}
            />
            <MetricItem
                id="supplier-api-connections-metric-pending"
                label="待配置"
                value={data?.metrics.pendingConfig ?? 0}
            />
            <MetricItem
                id="supplier-api-connections-metric-health"
                label="健康异常"
                value={data?.metrics.healthAbnormal ?? 0}
            />
            <MetricItem
                id="supplier-api-connections-metric-catalog"
                label="目录陈旧"
                value={data?.metrics.catalogStale ?? 0}
            />
        </MetricStrip>
    )
}
