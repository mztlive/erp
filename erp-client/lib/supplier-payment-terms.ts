/** 自然周期付款条件的页面编解码；旧货到条件继续由原码表解析。 */
export const PERIODIC_SETTLEMENTS = [
    { value: "weekly", label: "周结", period: "WEEK" },
    { value: "monthly", label: "月结", period: "MONTH" },
    { value: "quarterly", label: "季结", period: "QUARTER" },
    { value: "half_yearly", label: "半年结", period: "HALF_YEAR" },
    { value: "yearly", label: "年结", period: "YEAR" },
] as const

export const periodicSettlement = (value: string) =>
    PERIODIC_SETTLEMENTS.find(
        (entry) => entry.value === value || entry.label === value,
    )
export const parsePeriodicTerm = (value: string) => {
    const label =
        /^(周结|月结|季结|半年结|年结)，期末后 (\d{1,3}) 天付款$/.exec(
            value.trim(),
        )
    const normalized = label
        ? `PERIOD_${periodicSettlement(label[1])!.period}_${label[2]}`
        : value.trim().toUpperCase()
    const match = /^PERIOD_(WEEK|MONTH|QUARTER|HALF_YEAR|YEAR)_(\d{1,3})$/.exec(
        normalized,
    )
    if (!match || Number(match[2]) > 366) return undefined
    const settlement = PERIODIC_SETTLEMENTS.find(
        (entry) => entry.period === match[1],
    )!
    return {
        ...settlement,
        days: Number(match[2]),
        code: `PERIOD_${match[1]}_${Number(match[2])}`,
    }
}
export const periodicPaymentTerm = (settlement: string, days = "15") => {
    const entry = periodicSettlement(settlement)
    return entry ? `PERIOD_${entry.period}_${days}` : ""
}
export const RECONCILIATION_CYCLES = [
    { value: "daily", label: "每日对账" },
    { value: "weekly", label: "每周对账" },
    { value: "monthly", label: "每月对账" },
    { value: "quarterly", label: "每季对账" },
    { value: "half_yearly", label: "每半年对账" },
    { value: "yearly", label: "每年对账" },
    { value: "none", label: "无固定周期" },
] as const

export const reconciliationCycle = (value: string) =>
    RECONCILIATION_CYCLES.find(
        (cycle) => cycle.value === value || cycle.label === value,
    )?.value

export const reconciliationCycleLabel = (value?: string | null) =>
    RECONCILIATION_CYCLES.find((cycle) => cycle.value === value)?.label
