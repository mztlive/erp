import {
    compactFixed,
    formatCurrencyFixed,
    formatFixedDisplay,
    formatScaled,
    normalizeFixed,
    parseDecimal,
} from "@/lib/fixed-decimal"

export type MoneyMagnitude = "base" | "wan" | "million" | "yi"

/** 金额只接受十进制字符串；不接收已格式化的单位、货币符号或科学计数法。 */
export function describeMoney(value: string | null | undefined) {
    if (value == null) return null
    const match = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(value.trim())
    if (!match) return null
    const fraction = match[3] ?? ""
    const canonical = `${match[1] === "-" ? "-" : ""}${match[2].replace(/^0+(?=\d)/, "")}${fraction ? `.${fraction}` : ""}`
    const maxScale = Math.max(2, fraction.length)
    const parsed = parseDecimal(canonical, { maxScale, allowNegative: true })
    const absolute =
        parsed.unscaled < BigInt(0) ? -parsed.unscaled : parsed.unscaled
    const scaleFactor = BigInt(10) ** BigInt(parsed.scale)
    const integer = absolute / scaleFactor
    const magnitude: MoneyMagnitude =
        integer >= BigInt(100000000)
            ? "yi"
            : integer >= BigInt(1000000)
              ? "million"
              : integer >= BigInt(10000)
                ? "wan"
                : "base"
    // 精确值至少两位小数；单价等高精度值保留全部有效小数。
    const exact = formatCurrencyFixed(canonical, {
        maxScale,
        minimumFractionDigits: 2,
    })
    const copyValue = formatFixedDisplay(canonical, {
        maxScale,
        minimumFractionDigits: 2,
        useGrouping: false,
    })
    const readout = moneyReadout(copyValue)
    if (magnitude === "base") {
        return { exact, copyValue, readout, magnitude, compact: null }
    }

    const unitScale = magnitude === "yi" ? 8 : 4
    const shifted = formatScaled(parsed.unscaled, parsed.scale + unitScale)
    const rounded = normalizeFixed(shifted, {
        maxScale: parsed.scale + unitScale,
        outputScale: 2,
        allowNegative: true,
    })
    const roundedDecimal = parseDecimal(rounded, {
        maxScale: 2,
        allowNegative: true,
    })
    const roundedAbsolute =
        roundedDecimal.unscaled < BigInt(0)
            ? -roundedDecimal.unscaled
            : roundedDecimal.unscaled
    const represented =
        roundedAbsolute * BigInt(10) ** BigInt(parsed.scale + unitScale)
    const original = absolute * BigInt(100)
    // 不让舍入跨过颜色档位或亿元单位边界。
    const nextBoundary =
        magnitude === "wan"
            ? BigInt(1000000)
            : magnitude === "million"
              ? BigInt(100000000)
              : null
    const crossesBoundary =
        nextBoundary != null &&
        roundedAbsolute * BigInt(10) ** BigInt(unitScale) >=
            nextBoundary * BigInt(100)
    const compact = crossesBoundary
        ? null
        : `${represented !== original ? "≈" : ""}${formatFixedDisplay(rounded, { maxScale: 2 })} ${unitScale === 8 ? "亿元" : "万元"}`
    return { exact, copyValue, readout, magnitude, compact }
}

/** 输入辅助读数保留全部精度；不改变输入框的元单位与提交值。 */
function moneyReadout(value: string): string {
    const negative = value.startsWith("-")
    const [whole, fraction = ""] = compactFixed(value.replace(/^-/, "")).split(
        ".",
    )
    const integer = BigInt(whole)
    const yi = integer / BigInt(100000000)
    const wan = (integer % BigInt(100000000)) / BigInt(10000)
    const yuan = integer % BigInt(10000)
    const groups = `${yi ? `${yi}亿` : ""}${wan ? `${wan}万` : ""}`
    const prefix = negative ? "负" : ""
    if (fraction.length > 2) return `${prefix}${groups}${yuan}.${fraction}元`
    const jiao = fraction[0] && fraction[0] !== "0" ? `${fraction[0]}角` : ""
    const fen = fraction[1] && fraction[1] !== "0" ? `${fraction[1]}分` : ""
    return `${prefix}${groups}${yuan || !groups ? yuan : ""}元${jiao}${fen}`
}
