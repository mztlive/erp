import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
} from "@/lib/fixed-decimal"

/** 常用税率是百分数列表，空值保持未登记；不提供任何默认税率。 */
export const parseSupplierTaxRates = (raw: string): string[] => {
    if (!raw.trim()) return []
    const parts = raw
        .trim()
        .split(/[、,，;；\s]+/)
        .filter(Boolean)
    if (parts.length > 32) throw new Error("常用进项税率不能超过 32 项")
    const rates = parts.map((part) => {
        const value = part.replace(/[%％]$/, "")
        if (!/^(?:0|[1-9]\d?)(?:\.\d{1,4})?$/.test(value))
            throw new Error("请输入有效税率，多个税率用顿号分隔，如 9%、13%")
        return compactFixed(
            divideFixed(value, "100", {
                numeratorMaxScale: 4,
                denominatorMaxScale: 0,
                outputScale: 6,
            }),
        )
    })
    return [...new Set(rates)].sort((a, b) => compareDecimal(a, b, 6))
}
export const supplierTaxPercentages = (
    rates?: readonly string[] | null,
    legacy?: string | null,
) =>
    (rates ?? (legacy ? [legacy] : []))
        .map((rate) =>
            compactFixed(
                multiplyFixed(rate, "100", {
                    leftMaxScale: 6,
                    rightMaxScale: 0,
                    outputScale: 4,
                }),
            ),
        )
        .join("、")
