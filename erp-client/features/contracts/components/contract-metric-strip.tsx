"use client"

import {
    MetricFilterItem,
    MetricStrip,
} from "@/components/business"
import {
    computeContractMetrics,
    type ContractMetricFilter,
} from "@/features/contracts/lib/filter-contracts"

type ContractMetricStripProps = {
    metrics: ReturnType<typeof computeContractMetrics>
    active: ContractMetricFilter
    onChange: (metric: ContractMetricFilter) => void
}

/** 合同快速筛选指标条：全部 / 有效 / 30 天内到期 / 已到期 / 已终止。 */
export function ContractMetricStrip({
    metrics,
    active,
    onChange,
}: ContractMetricStripProps) {
    return (
        <MetricStrip columns={5} aria-label="合同快速筛选">
            <MetricFilterItem
                label="全部合同"
                value={metrics.all}
                detail="当前业务范围"
                active={active === "all"}
                onClick={() => onChange("all")}
            />
            <MetricFilterItem
                label="有效"
                value={metrics.effective}
                detail="可关联建单"
                active={active === "effective"}
                onClick={() => onChange("effective")}
            />
            <MetricFilterItem
                label="30 天内到期"
                value={metrics.expiring_30d}
                detail="将到期提醒"
                active={active === "expiring_30d"}
                onClick={() => onChange("expiring_30d")}
            />
            <MetricFilterItem
                label="已到期"
                value={metrics.expired}
                detail="历史可追溯"
                active={active === "expired"}
                onClick={() => onChange("expired")}
            />
            <MetricFilterItem
                label="已终止"
                value={metrics.terminated}
                detail="不再履行"
                active={active === "terminated"}
                onClick={() => onChange("terminated")}
            />
        </MetricStrip>
    )
}
