import { formatCurrencyFixed } from "@/lib/fixed-decimal"

export function shortHash(hash: string): string {
    if (hash.length <= 20) return hash
    return `${hash.slice(0, 12)}…${hash.slice(-6)}`
}

export function formatMoney(value: string): string {
    try {
        return formatCurrencyFixed(value, {
            maxScale: 6,
            minimumFractionDigits: 2,
            maximumFractionDigits: 2,
        })
    } catch {
        return "¥0.00"
    }
}
