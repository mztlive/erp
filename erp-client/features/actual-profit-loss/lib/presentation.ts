import { formatCurrencyFixed } from "@/lib/fixed-decimal"

export const PROFIT_LOSS_SCOPE_LABEL = "非卡券 · 不含税"

export function formatMoneyDisplay(value: string | undefined | null): string {
    if (value == null || value === "" || value === "—") return "—"
    try {
        return formatCurrencyFixed(value, {
            maxScale: 6,
            minimumFractionDigits: 2,
            maximumFractionDigits: 2,
        })
    } catch {
        return value
    }
}
