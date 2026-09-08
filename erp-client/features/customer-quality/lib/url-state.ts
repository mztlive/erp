import type { BusinessTypeFilter, CustomerQualityScenario } from "../types"

const SCENARIOS = new Set<CustomerQualityScenario>([
    "default",
    "no_period_default",
    "empty",
    "no_scope",
    "forbidden",
    "field_denied",
    "stale",
    "rebuilding",
    "failed",
    "refresh_failed",
])

export function parseScenario(
    raw: string | null,
): CustomerQualityScenario | undefined {
    if (raw && SCENARIOS.has(raw as CustomerQualityScenario)) {
        return raw as CustomerQualityScenario
    }
    return undefined
}

export function parseBusinessType(
    raw: string | null,
): BusinessTypeFilter | undefined {
    if (raw === "VOUCHER" || raw === "GOODS_SERVICE") return raw
    return undefined
}
