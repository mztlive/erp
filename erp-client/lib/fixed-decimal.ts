/**
 * Decimal-string helpers for fixed-point arithmetic.
 * arithmetic. Business amounts must never pass through JavaScript `number`.
 */

export type ParsedDecimal = Readonly<{
    unscaled: bigint
    scale: number
}>

const ZERO = BigInt(0)
const ONE = BigInt(1)
const TEN = BigInt(10)

function powerOfTen(scale: number): bigint {
    if (!Number.isInteger(scale) || scale < 0) {
        throw new Error("INVALID_DECIMAL_SCALE")
    }
    return TEN ** BigInt(scale)
}

/** Parse a canonical decimal string without currency symbols, separators or `%`. */
export function parseDecimal(
    value: string,
    options: { maxScale: number; allowNegative?: boolean },
): ParsedDecimal {
    const normalized = value.trim()
    const pattern = options.allowNegative
        ? /^-?(?:0|[1-9]\d*)(?:\.(\d+))?$/
        : /^(?:0|[1-9]\d*)(?:\.(\d+))?$/
    const match = normalized.match(pattern)
    if (!match) throw new Error("INVALID_DECIMAL")

    const fraction = match[1] ?? ""
    if (fraction.length > options.maxScale) {
        throw new Error("DECIMAL_SCALE_EXCEEDED")
    }

    const negative = normalized.startsWith("-")
    const unsigned = negative ? normalized.slice(1) : normalized
    const [integerPart, fractionPart = ""] = unsigned.split(".")
    const digits = `${integerPart}${fractionPart}`
    const unscaled = BigInt(digits || "0") * (negative ? -ONE : ONE)
    return { unscaled, scale: fractionPart.length }
}

function roundDivide(numerator: bigint, denominator: bigint): bigint {
    if (denominator === ZERO) throw new Error("INVALID_DECIMAL_DIVISOR")
    const negative = numerator < ZERO !== denominator < ZERO
    const absoluteNumerator = numerator < ZERO ? -numerator : numerator
    const absoluteDenominator = denominator < ZERO ? -denominator : denominator
    const quotient = absoluteNumerator / absoluteDenominator
    const remainder = absoluteNumerator % absoluteDenominator
    const doubledRemainder = remainder * BigInt(2)
    const rounded =
        doubledRemainder > absoluteDenominator ||
        (doubledRemainder === absoluteDenominator &&
            quotient % BigInt(2) !== ZERO)
            ? quotient + ONE
            : quotient
    return negative ? -rounded : rounded
}

function rescale(decimal: ParsedDecimal, targetScale: number): bigint {
    if (decimal.scale === targetScale) return decimal.unscaled
    if (decimal.scale < targetScale) {
        return decimal.unscaled * powerOfTen(targetScale - decimal.scale)
    }
    return roundDivide(
        decimal.unscaled,
        powerOfTen(decimal.scale - targetScale),
    )
}

/** Format a scaled integer as a canonical fixed-point decimal string. */
export function formatScaled(unscaled: bigint, scale: number): string {
    const negative = unscaled < ZERO
    const absolute = negative ? -unscaled : unscaled
    const digits = absolute.toString().padStart(scale + 1, "0")
    const value =
        scale === 0
            ? digits
            : `${digits.slice(0, -scale)}.${digits.slice(-scale)}`
    return negative && absolute !== ZERO ? `-${value}` : value
}

/** Remove insignificant fractional zeroes without changing the decimal value. */
export function compactFixed(value: string): string {
    if (!value.includes(".")) return value
    const compact = value.replace(/0+$/, "").replace(/\.$/, "")
    return compact === "-0" ? "0" : compact
}

export type FixedDisplayOptions = Readonly<{
    maxScale: number
    minimumFractionDigits?: number
    maximumFractionDigits?: number
    locale?: string
    useGrouping?: boolean
}>

/**
 * Format a decimal string for display without converting the business value to
 * JavaScript `number`. Integer grouping is delegated to Intl using `bigint`.
 */
export function formatFixedDisplay(
    value: string,
    options: FixedDisplayOptions,
): string {
    const minimumFractionDigits = options.minimumFractionDigits ?? 0
    const maximumFractionDigits =
        options.maximumFractionDigits ?? options.maxScale
    if (
        minimumFractionDigits < 0 ||
        maximumFractionDigits < minimumFractionDigits ||
        maximumFractionDigits > options.maxScale
    ) {
        throw new Error("INVALID_DECIMAL_DISPLAY_SCALE")
    }

    const parsed = parseDecimal(value, {
        maxScale: options.maxScale,
        allowNegative: true,
    })
    const targetScale = Math.min(
        Math.max(parsed.scale, minimumFractionDigits),
        maximumFractionDigits,
    )
    const normalized = formatScaled(rescale(parsed, targetScale), targetScale)
    const negative = normalized.startsWith("-")
    const unsigned = negative ? normalized.slice(1) : normalized
    const [integerPart, rawFraction = ""] = unsigned.split(".")
    let fraction = rawFraction
    while (fraction.length > minimumFractionDigits && fraction.endsWith("0")) {
        fraction = fraction.slice(0, -1)
    }
    const groupedInteger = new Intl.NumberFormat(options.locale ?? "zh-CN", {
        useGrouping: options.useGrouping ?? true,
        maximumFractionDigits: 0,
    }).format(BigInt(integerPart))
    return `${negative ? "-" : ""}${groupedInteger}${fraction ? `.${fraction}` : ""}`
}

/** Format a currency amount while preserving the decimal string exactly. */
export function formatCurrencyFixed(
    value: string,
    options: FixedDisplayOptions & { symbol?: string },
): string {
    const formatted = formatFixedDisplay(value, options)
    const symbol = options.symbol ?? "¥"
    return formatted.startsWith("-")
        ? `-${symbol}${formatted.slice(1)}`
        : `${symbol}${formatted}`
}

/** Normalize and round a decimal string to an exact output scale. */
export function normalizeFixed(
    value: string,
    options: { maxScale: number; outputScale: number; allowNegative?: boolean },
): string {
    return formatScaled(
        rescale(
            parseDecimal(value, {
                maxScale: options.maxScale,
                allowNegative: options.allowNegative,
            }),
            options.outputScale,
        ),
        options.outputScale,
    )
}

/** Multiply two non-negative decimals and round the result to `outputScale`. */
export function multiplyFixed(
    left: string,
    right: string,
    options: {
        leftMaxScale: number
        rightMaxScale: number
        outputScale: number
    },
): string {
    const a = parseDecimal(left, { maxScale: options.leftMaxScale })
    const b = parseDecimal(right, { maxScale: options.rightMaxScale })
    return formatScaled(
        rescale(
            { unscaled: a.unscaled * b.unscaled, scale: a.scale + b.scale },
            options.outputScale,
        ),
        options.outputScale,
    )
}

/** Divide two decimals and round the quotient to `outputScale` using half-even rounding. */
export function divideFixed(
    numerator: string,
    denominator: string,
    options: {
        numeratorMaxScale: number
        denominatorMaxScale: number
        outputScale: number
        allowNegative?: boolean
    },
): string {
    const a = parseDecimal(numerator, {
        maxScale: options.numeratorMaxScale,
        allowNegative: options.allowNegative,
    })
    const b = parseDecimal(denominator, {
        maxScale: options.denominatorMaxScale,
        allowNegative: options.allowNegative,
    })
    if (b.unscaled === ZERO) throw new Error("INVALID_DECIMAL_DIVISOR")
    const scaledNumerator =
        a.unscaled * powerOfTen(b.scale + options.outputScale)
    const scaledDenominator = b.unscaled * powerOfTen(a.scale)
    return formatScaled(
        roundDivide(scaledNumerator, scaledDenominator),
        options.outputScale,
    )
}

/** Sum decimal strings after normalizing every input to the same scale. */
export function sumFixed(
    values: readonly string[],
    options: { maxScale: number; outputScale: number; allowNegative?: boolean },
): string {
    const total = values.reduce(
        (sum, value) =>
            sum +
            rescale(
                parseDecimal(value, {
                    maxScale: options.maxScale,
                    allowNegative: options.allowNegative,
                }),
                options.outputScale,
            ),
        ZERO,
    )
    return formatScaled(total, options.outputScale)
}

/** Subtract two decimals and normalize the result to `outputScale`. */
export function subtractFixed(
    left: string,
    right: string,
    options: { maxScale: number; outputScale: number },
): string {
    return sumFixed([left, `-${right}`], {
        ...options,
        allowNegative: true,
    })
}

/** Return the smaller decimal after normalizing both values to `outputScale`. */
export function minFixed(
    left: string,
    right: string,
    options: { maxScale: number; outputScale: number; allowNegative?: boolean },
): string {
    const smaller =
        compareDecimal(left, right, options.maxScale) <= 0 ? left : right
    return normalizeFixed(smaller, options)
}

/** Clamp a decimal to zero and normalize it to `outputScale`. */
export function clampZeroFixed(
    value: string,
    options: { maxScale: number; outputScale: number },
): string {
    return compareDecimal(value, "0", options.maxScale) < 0
        ? formatScaled(ZERO, options.outputScale)
        : normalizeFixed(value, { ...options, allowNegative: true })
}

/** Derive line net/tax amounts from a gross amount and a percentage tax rate. */
export function splitGrossByPercentRate(
    grossAmount: string,
    taxRatePercent: string,
): { gross: string; net: string; tax: string } {
    const gross = parseDecimal(grossAmount, { maxScale: 2 })
    const grossCents = rescale(gross, 2)
    const rate = parseDecimal(taxRatePercent, { maxScale: 6 })
    const percentageBase = BigInt(100) * powerOfTen(rate.scale)
    const netCents = roundDivide(
        grossCents * percentageBase,
        percentageBase + rate.unscaled,
    )
    return {
        gross: formatScaled(grossCents, 2),
        net: formatScaled(netCents, 2),
        tax: formatScaled(grossCents - netCents, 2),
    }
}

/** Compare two canonical decimal strings without converting them to `number`. */
export function compareDecimal(
    left: string,
    right: string,
    maxScale: number,
): -1 | 0 | 1 {
    const a = parseDecimal(left, { maxScale, allowNegative: true })
    const b = parseDecimal(right, { maxScale, allowNegative: true })
    const scale = Math.max(a.scale, b.scale)
    const av = rescale(a, scale)
    const bv = rescale(b, scale)
    return av < bv ? -1 : av > bv ? 1 : 0
}
