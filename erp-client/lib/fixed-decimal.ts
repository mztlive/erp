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
  options: { maxScale: number; allowNegative?: boolean }
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
  if (denominator <= ZERO) throw new Error("INVALID_DECIMAL_DIVISOR")
  const negative = numerator < ZERO
  const absolute = negative ? -numerator : numerator
  const quotient = absolute / denominator
  const remainder = absolute % denominator
  const rounded = remainder * BigInt(2) >= denominator ? quotient + ONE : quotient
  return negative ? -rounded : rounded
}

function rescale(decimal: ParsedDecimal, targetScale: number): bigint {
  if (decimal.scale === targetScale) return decimal.unscaled
  if (decimal.scale < targetScale) {
    return decimal.unscaled * powerOfTen(targetScale - decimal.scale)
  }
  return roundDivide(
    decimal.unscaled,
    powerOfTen(decimal.scale - targetScale)
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

/** Normalize a decimal string while keeping no unnecessary trailing zeros. */
export function canonicalDecimal(
  value: string,
  options: { maxScale: number; allowNegative?: boolean }
): string {
  const parsed = parseDecimal(value, options)
  if (parsed.scale === 0) return formatScaled(parsed.unscaled, 0)
  const fixed = formatScaled(parsed.unscaled, parsed.scale)
  return fixed.replace(/\.0+$/, "").replace(/(\.\d*?)0+$/, "$1")
}

/** Normalize and round a decimal string to an exact output scale. */
export function normalizeFixed(
  value: string,
  options: { maxScale: number; outputScale: number; allowNegative?: boolean }
): string {
  return formatScaled(
    rescale(
      parseDecimal(value, {
        maxScale: options.maxScale,
        allowNegative: options.allowNegative,
      }),
      options.outputScale
    ),
    options.outputScale
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
  }
): string {
  const a = parseDecimal(left, { maxScale: options.leftMaxScale })
  const b = parseDecimal(right, { maxScale: options.rightMaxScale })
  return formatScaled(
    rescale(
      { unscaled: a.unscaled * b.unscaled, scale: a.scale + b.scale },
      options.outputScale
    ),
    options.outputScale
  )
}

/** Sum decimal strings after normalizing every input to the same scale. */
export function sumFixed(
  values: readonly string[],
  options: { maxScale: number; outputScale: number; allowNegative?: boolean }
): string {
  const total = values.reduce(
    (sum, value) =>
      sum +
      rescale(
        parseDecimal(value, {
          maxScale: options.maxScale,
          allowNegative: options.allowNegative,
        }),
        options.outputScale
      ),
    ZERO
  )
  return formatScaled(total, options.outputScale)
}

/** Derive line net/tax amounts from a gross amount and a fractional tax rate. */
export function splitGrossByFractionRate(
  grossAmount: string,
  taxRate: string
): { gross: string; net: string; tax: string } {
  const gross = parseDecimal(grossAmount, { maxScale: 2 })
  const grossCents = rescale(gross, 2)
  const rate = parseDecimal(taxRate, { maxScale: 6 })
  const rateBase = powerOfTen(rate.scale)
  const netCents = roundDivide(grossCents * rateBase, rateBase + rate.unscaled)
  return {
    gross: formatScaled(grossCents, 2),
    net: formatScaled(netCents, 2),
    tax: formatScaled(grossCents - netCents, 2),
  }
}

/** Derive line net/tax amounts from a gross amount and a percentage tax rate. */
export function splitGrossByPercentRate(
  grossAmount: string,
  taxRatePercent: string
): { gross: string; net: string; tax: string } {
  const gross = parseDecimal(grossAmount, { maxScale: 2 })
  const grossCents = rescale(gross, 2)
  const rate = parseDecimal(taxRatePercent, { maxScale: 6 })
  const percentageBase = BigInt(100) * powerOfTen(rate.scale)
  const netCents = roundDivide(
    grossCents * percentageBase,
    percentageBase + rate.unscaled
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
  maxScale: number
): -1 | 0 | 1 {
  const a = parseDecimal(left, { maxScale, allowNegative: true })
  const b = parseDecimal(right, { maxScale, allowNegative: true })
  const scale = Math.max(a.scale, b.scale)
  const av = rescale(a, scale)
  const bv = rescale(b, scale)
  return av < bv ? -1 : av > bv ? 1 : 0
}
