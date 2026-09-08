"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import { computeContractMetrics } from "@/features/contracts/lib/filter-contracts"

export function ContractMetricStrip({
    metrics,
}: {
    metrics: ReturnType<typeof computeContractMetrics>
}) {
    return (
        <MetricStrip columns={5} aria-label="合同指标">
            <MetricItem
                id="card-contracts-list-metric-all"
                label="全部合同"
                value={metrics.all}
                detail="当前业务范围"
                detailMode="tooltip"
            />
            <MetricItem
                id="card-contracts-list-metric-effective"
                label="有效"
                value={metrics.effective}
                detail="可关联建单"
                detailMode="tooltip"
            />
            <MetricItem
                id="card-contracts-list-metric-expiring-30d"
                label="30 天内到期"
                value={metrics.expiring_30d}
                detail="将到期提醒"
                detailMode="tooltip"
            />
            <MetricItem
                id="card-contracts-list-metric-expired"
                label="已到期"
                value={metrics.expired}
                detail="历史可追溯"
                detailMode="tooltip"
            />
            <MetricItem
                id="card-contracts-list-metric-terminated"
                label="已终止"
                value={metrics.terminated}
                detail="不再履行"
                detailMode="tooltip"
            />
        </MetricStrip>
    )
}
