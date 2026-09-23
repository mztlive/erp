import { formatCurrencyFixed } from "@/lib/fixed-decimal"

export const PROFIT_LOSS_SCOPE_LABEL = "非卡券 · 不含税"

/**
 * 成本类型权威字典，对齐 `erp-finance` 的 `CostType::as_str` / `label`。
 * 筛选使用完整集合，不从报表成本构成归纳。
 */
export const COST_TYPE_OPTIONS: readonly { value: string; label: string }[] = [
    { value: "product", label: "商品" },
    { value: "logistics", label: "物流" },
    { value: "printing", label: "印刷" },
    { value: "storage", label: "仓储" },
    { value: "delivery", label: "配送" },
    { value: "platform_tech", label: "平台技术" },
    { value: "offline_service", label: "线下服务" },
    { value: "rebate", label: "返点" },
    { value: "other", label: "其他" },
]

export const COST_TYPE_LABEL: Record<string, string> = Object.fromEntries(
    COST_TYPE_OPTIONS.map((option) => [option.value, option.label]),
)

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
