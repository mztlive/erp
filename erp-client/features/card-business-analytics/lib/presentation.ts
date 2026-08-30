import { formatCurrencyFixed } from "@/lib/fixed-decimal"

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
