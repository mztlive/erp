import type {
    CardBusinessDimension,
    CostBasisCode,
    CoverageFilter,
    DateBasis,
    ExpiryState,
    PeriodPreset,
} from "../types"

export function parseDateBasis(raw: string | null): DateBasis | "" {
    if (raw === "consumption" || raw === "sales" || raw === "expiry") return raw
    return ""
}

export function parseDimension(raw: string | null): CardBusinessDimension {
    if (
        raw === "customer" ||
        raw === "sales_order" ||
        raw === "voucher_category" ||
        raw === "card_instance"
    ) {
        return raw
    }
    return "customer"
}

export function parsePreset(raw: string | null): PeriodPreset {
    if (
        raw === "last-month" ||
        raw === "quarter-to-date" ||
        raw === "month-to-date"
    ) {
        return raw
    }
    return "month-to-date"
}

export function parseExpiry(raw: string | null): ExpiryState {
    if (raw === "active" || raw === "expired" || raw === "all") return raw
    return "all"
}

export function parseCoverage(raw: string | null): CoverageFilter {
    if (raw === "below_threshold" || raw === "none" || raw === "all") return raw
    return "all"
}

export function parseCostBasis(
    raw: string | null,
): CostBasisCode[] | undefined {
    if (!raw) return undefined
    const parts = raw
        .split(",")
        .map((s) => s.trim())
        .filter(
            (s): s is CostBasisCode =>
                s === "ACTUAL" || s === "STANDARD" || s === "NONE",
        )
    return parts.length > 0 ? parts : undefined
}

/** periodPreset → 相对当前日期计算 from/to。 */
export function resolvePeriod(preset: PeriodPreset): {
    from: string
    to: string
} {
    const today = new Date()
    const iso = (d: Date): string => {
        const y = d.getFullYear()
        const m = `${d.getMonth() + 1}`.padStart(2, "0")
        const day = `${d.getDate()}`.padStart(2, "0")
        return `${y}-${m}-${day}`
    }
    if (preset === "last-month") {
        const last = new Date(today.getFullYear(), today.getMonth(), 0)
        const first = new Date(last.getFullYear(), last.getMonth(), 1)
        return { from: iso(first), to: iso(last) }
    }
    if (preset === "quarter-to-date") {
        const qStart = new Date(
            today.getFullYear(),
            Math.floor(today.getMonth() / 3) * 3,
            1,
        )
        return { from: iso(qStart), to: iso(today) }
    }
    return { from: iso(today), to: iso(today) }
}
