export type ContractMetricFilter =
    | "all"
    | "effective"
    | "expiring_30d"
    | "expired"
    | "terminated"

export function contractMetricLabel(key: ContractMetricFilter): string {
    switch (key) {
        case "effective":
            return "有效"
        case "expiring_30d":
            return "30 天内到期"
        case "expired":
            return "已到期"
        case "terminated":
            return "已终止"
        default:
            return "全部"
    }
}

/** 由服务端按可见合同范围返回的指标。 */
export type ContractMetrics = Record<ContractMetricFilter, number>
export const EMPTY_CONTRACT_METRICS: ContractMetrics = {
    all: 0,
    effective: 0,
    expiring_30d: 0,
    expired: 0,
    terminated: 0,
}
