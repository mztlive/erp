export const PROFIT_LOSS_SCOPE_LABEL = "非卡券 · 不含税"

export function formatMoneyDisplay(value: string | undefined | null): string {
    if (value == null || value === "" || value === "—") return "—"
    const amount = Number(value)
    if (!Number.isFinite(amount)) return value
    return new Intl.NumberFormat("zh-CN", {
        style: "currency",
        currency: "CNY",
        minimumFractionDigits: 2,
    }).format(amount)
}
