const money = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
})

export function shortHash(hash: string): string {
    if (hash.length <= 20) return hash
    return `${hash.slice(0, 12)}…${hash.slice(-6)}`
}

export function formatMoney(value: string): string {
    return money.format(Number(value) || 0)
}

export function moneyStrSafe(value: number): string {
    if (!Number.isFinite(value)) return "0.00"
    return value.toFixed(2)
}
