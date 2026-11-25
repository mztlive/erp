"use client"

import type * as React from "react"
import { TriangleAlertIcon } from "lucide-react"

import {
    compareDecimal,
    divideFixed,
    formatScaled,
    normalizeFixed,
    parseDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

const ZERO = BigInt(0)
const TEN = BigInt(10)
const HUNDRED = BigInt(100)
const THOUSAND = BigInt(1000)
const RATIO_SCALE = 6

export type MicroBarHeight = "sm" | "default" | "md"

const barHeightClass: Record<MicroBarHeight, string> = {
    sm: "h-1",
    default: "h-1.5",
    md: "h-2",
}

function parseToScaledBigInt(
    raw: number | string | bigint | null | undefined,
    allowNegative = false,
): bigint | null {
    if (raw === null || raw === undefined) return null
    if (typeof raw === "bigint") {
        if (!allowNegative && raw < ZERO) return null
        return raw * TEN ** BigInt(RATIO_SCALE)
    }
    if (typeof raw === "number") {
        if (!Number.isFinite(raw)) return null
        if (!allowNegative && raw < 0) return null
        if (Number.isInteger(raw)) {
            return BigInt(raw) * TEN ** BigInt(RATIO_SCALE)
        }
        try {
            const parsed = parseDecimal(raw.toFixed(RATIO_SCALE), {
                maxScale: RATIO_SCALE,
                allowNegative,
            })
            return parsed.unscaled * TEN ** BigInt(RATIO_SCALE - parsed.scale)
        } catch {
            return null
        }
    }
    const trimmed = raw.trim()
    if (!trimmed) return null
    try {
        const parsed = parseDecimal(trimmed, {
            maxScale: RATIO_SCALE,
            allowNegative,
        })
        return parsed.unscaled * TEN ** BigInt(RATIO_SCALE - parsed.scale)
    } catch {
        return null
    }
}

function ratioToPercentTenths(numerator: bigint, denominator: bigint): string {
    if (denominator <= ZERO || numerator <= ZERO) return "0.0"
    const tenths =
        (numerator * THOUSAND + denominator / BigInt(2)) / denominator
    const clamped = tenths > THOUSAND ? THOUSAND : tenths
    const whole = clamped / TEN
    const frac = clamped % TEN
    return `${whole.toString()}.${frac.toString()}`
}

function safeNonNegativePercent(raw: string | null | undefined): string | null {
    if (!raw?.trim()) return null
    try {
        const normalized = normalizeFixed(raw, {
            maxScale: 4,
            outputScale: 1,
            allowNegative: true,
        })
        return compareDecimal(normalized, "0", 1) <= 0 ? "0.0" : normalized
    } catch {
        return null
    }
}

export type CoverageRatioTone =
    | "full"
    | "partial"
    | "over"
    | "uncovered"
    | "info"

export type CoverageRatioSegment = Readonly<{
    key: string
    label: string
    /** 非负计数或十进制权重（如行项数、数量字符串、BigInt）。 */
    value: number | string | bigint
    tone: CoverageRatioTone
    /** 可选图例数值覆盖展示，默认显示 `value`。 */
    valueText?: React.ReactNode
}>

const coverageToneStyles: Readonly<
    Record<
        CoverageRatioTone,
        Readonly<{
            barClass: string
            dotClass: string
            textClass: string
        }>
    >
> = {
    full: {
        barClass: "bg-success",
        dotClass: "bg-success",
        textClass: "text-muted-foreground",
    },
    partial: {
        barClass: "bg-warning",
        dotClass: "bg-warning",
        textClass: "font-medium text-warning-soft-foreground",
    },
    over: {
        barClass: "bg-destructive",
        dotClass: "bg-destructive",
        textClass: "font-medium text-destructive-soft-foreground",
    },
    info: {
        barClass: "bg-info",
        dotClass: "bg-info",
        textClass: "text-muted-foreground",
    },
    uncovered: {
        barClass: "bg-muted",
        dotClass: "border border-muted-foreground/55 bg-muted",
        textClass: "text-muted-foreground",
    },
}

export interface CoverageRatioBarProps extends Omit<
    React.ComponentProps<"div">,
    "children"
> {
    /** 满配项数（`bg-success`）。 */
    full?: number
    /** 部分覆盖项数（`bg-warning`）。 */
    partial?: number
    /** 超配或不匹配项数（`bg-destructive`）。 */
    over?: number
    /** 未覆盖项数（`bg-muted`）。 */
    uncovered?: number
    /** 自定义分段列表（传入时优先于 `full / partial / over / uncovered`）。 */
    segments?: readonly CoverageRatioSegment[]
    /** 覆盖标准四态默认中文图例名称。 */
    labels?: Partial<Record<"full" | "partial" | "over" | "uncovered", string>>
    /** 可选顶部左侧说明（如「明细覆盖」）。 */
    label?: React.ReactNode
    /** 可选顶部右侧汇总（如「4/5 行 · 占额 82%」）。 */
    summary?: React.ReactNode
    /** 是否展示圆点图例，默认 `true`。 */
    showLegend?: boolean
    /** 微条高度：`sm`（4px）/ `default`（6px，`h-1.5`）/ `md`（8px）。 */
    height?: MicroBarHeight
    /** 进度条无障碍标签。 */
    ariaLabel?: string
}

/**
 * 覆配覆盖进度条（docs/task-page-design-system.md §3.3.3）：
 * 纯 CSS Flexbox + 语义令牌构建的 `h-1.5` 多段横向条，区分满配（`bg-success`）、
 * 部分（`bg-warning`）、超配/不匹配（`bg-destructive`）与未覆盖（`bg-muted`），配可选圆点图例。
 */
export function CoverageRatioBar({
    full = 0,
    partial = 0,
    over = 0,
    uncovered = 0,
    segments,
    labels,
    label,
    summary,
    showLegend = true,
    height = "default",
    ariaLabel = "覆配覆盖进度",
    className,
    ...props
}: CoverageRatioBarProps) {
    const rawSegments: readonly CoverageRatioSegment[] = segments ?? [
        {
            key: "full",
            label: labels?.full ?? "满配",
            value: full,
            tone: "full",
        },
        {
            key: "partial",
            label: labels?.partial ?? "部分",
            value: partial,
            tone: "partial",
        },
        {
            key: "over",
            label: labels?.over ?? "超配/异常",
            value: over,
            tone: "over",
        },
        {
            key: "uncovered",
            label: labels?.uncovered ?? "未覆盖",
            value: uncovered,
            tone: "uncovered",
        },
    ]

    const resolvedSegments = rawSegments
        .map((segment) => {
            const units = parseToScaledBigInt(segment.value, false)
            return units !== null && units > ZERO ? { ...segment, units } : null
        })
        .filter((entry): entry is CoverageRatioSegment & { units: bigint } =>
            Boolean(entry),
        )

    const totalUnits = resolvedSegments.reduce(
        (acc, segment) => acc + segment.units,
        ZERO,
    )

    if (totalUnits <= ZERO) {
        return null
    }

    const hasUncovered = resolvedSegments.some(
        (segment) => segment.tone === "uncovered",
    )
    const filledSegments = resolvedSegments.filter(
        (segment) => segment.tone !== "uncovered",
    )

    return (
        <div
            data-slot="coverage-ratio-bar"
            className={cn("min-w-0 space-y-1.5", className)}
            {...props}
        >
            {label != null || summary != null ? (
                <div className="flex flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5 text-xs">
                    {label != null ? (
                        <span className="text-muted-foreground">{label}</span>
                    ) : (
                        <span />
                    )}
                    {summary != null ? (
                        <span className="num font-medium text-foreground">
                            {summary}
                        </span>
                    ) : null}
                </div>
            ) : null}

            <div
                role="img"
                aria-label={ariaLabel}
                className={cn(
                    "flex w-full overflow-hidden rounded-full bg-muted",
                    barHeightClass[height],
                )}
            >
                {filledSegments.map((segment, index) => {
                    const widthPercent = `${ratioToPercentTenths(segment.units, totalUnits)}%`
                    const isLastFullSpan =
                        !hasUncovered && index === filledSegments.length - 1
                    return (
                        <div
                            key={segment.key}
                            title={`${segment.label}：${String(segment.value)} (${widthPercent})`}
                            className={cn(
                                "h-full border-r border-card/80 last:border-r-0",
                                coverageToneStyles[segment.tone].barClass,
                                isLastFullSpan && "flex-1",
                            )}
                            style={
                                isLastFullSpan
                                    ? undefined
                                    : { width: widthPercent }
                            }
                        />
                    )
                })}
            </div>

            {showLegend ? (
                <div className="num flex flex-wrap items-center gap-x-3 gap-y-1 text-tiny">
                    {resolvedSegments.map((segment) => {
                        const style = coverageToneStyles[segment.tone]
                        return (
                            <span
                                key={segment.key}
                                className={cn(
                                    "inline-flex items-center gap-1",
                                    style.textClass,
                                )}
                            >
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-1.5 shrink-0 rounded-full",
                                        style.dotClass,
                                    )}
                                />
                                <span>{segment.label}</span>
                                <span>
                                    {segment.valueText ?? segment.value}
                                </span>
                            </span>
                        )
                    })}
                </div>
            ) : null}
        </div>
    )
}

export interface CostMarginBarProps extends Omit<
    React.ComponentProps<"div">,
    "children"
> {
    /** 销售总额（十进制字符串，作为 100% 基准）；与预计算占比模式二选一。 */
    salesTotal?: string | null
    /** 货品采购成本（十进制字符串）。 */
    goodsCost?: string | null
    /** 附加费用（十进制字符串，可选）。 */
    extraCharges?: string | null
    /** 预计算货品采购成本占比（0~100 的十进制字符串，如 `"72.5"`）。 */
    goodsCostPercent?: string | null
    /** 预计算附加费用占比（0~100 的十进制字符串，如 `"5.2"`）。 */
    extraChargesPercent?: string | null
    /** 预计算毛利率或亏损率（十进制字符串，负值触发整单亏损态，如 `"-8.5"`）。 */
    marginPercent?: string | null
    /** 显式标记整单亏损；传金额模式时自动根据总成本 > 销售总额推导。 */
    isLoss?: boolean
    /** 整单亏损时覆盖默认溢亏比例文案。 */
    lossLabel?: React.ReactNode
    /** 可选顶部左侧说明。 */
    label?: React.ReactNode
    /** 可选顶部右侧汇总。 */
    summary?: React.ReactNode
    /** 是否展示分段百分比圆点图例（整单亏损时溢亏比例徽章始终展示），默认 `false`。 */
    showLegend?: boolean
    /** 微条高度：`sm`（4px）/ `default`（6px，`h-1.5`）/ `md`（8px）。 */
    height?: MicroBarHeight
    /** 自定义分段图例标签。 */
    labels?: Partial<{
        goodsCost: string
        extraCharges: string
        margin: string
        loss: string
    }>
    ariaLabel?: string
}

type ResolvedCostMarginModel = Readonly<{
    isLoss: boolean
    lossRatioPercent: string | null
    goodsPercent: string
    extraPercent: string
    marginPercent: string
}>

function resolveCostMarginFromAmounts(
    salesTotal: string,
    goodsCost: string | null | undefined,
    extraCharges: string | null | undefined,
    explicitLoss: boolean | undefined,
    explicitMarginPercent: string | null | undefined,
): ResolvedCostMarginModel | null {
    const salesScaled = parseToScaledBigInt(salesTotal, true)
    if (salesScaled === null) return null

    const rawGoods = parseToScaledBigInt(goodsCost ?? "0", true) ?? ZERO
    const rawExtra = parseToScaledBigInt(extraCharges ?? "0", true) ?? ZERO
    const goodsScaled = rawGoods > ZERO ? rawGoods : ZERO
    const extraScaled = rawExtra > ZERO ? rawExtra : ZERO
    const totalCostScaled = goodsScaled + extraScaled

    if (salesScaled <= ZERO) {
        if (explicitLoss || totalCostScaled > ZERO) {
            return {
                isLoss: true,
                lossRatioPercent: null,
                goodsPercent: "0.0",
                extraPercent: "0.0",
                marginPercent: "0.0",
            }
        }
        return null
    }

    const marginScaled = salesScaled - totalCostScaled
    const loss = explicitLoss ?? marginScaled < ZERO

    if (loss) {
        let lossRatioPercent: string | null = null
        if (explicitMarginPercent?.trim()) {
            try {
                const normalized = normalizeFixed(explicitMarginPercent, {
                    maxScale: 4,
                    outputScale: 1,
                    allowNegative: true,
                })
                lossRatioPercent = normalized.startsWith("-")
                    ? normalized.slice(1)
                    : normalized
            } catch {
                lossRatioPercent = null
            }
        } else if (marginScaled < ZERO) {
            try {
                const lossAbs = -marginScaled
                lossRatioPercent = divideFixed(
                    formatScaled(lossAbs * HUNDRED, RATIO_SCALE),
                    formatScaled(salesScaled, RATIO_SCALE),
                    {
                        numeratorMaxScale: RATIO_SCALE,
                        denominatorMaxScale: RATIO_SCALE,
                        outputScale: 1,
                    },
                )
            } catch {
                lossRatioPercent = null
            }
        }
        return {
            isLoss: true,
            lossRatioPercent,
            goodsPercent: "0.0",
            extraPercent: "0.0",
            marginPercent: "0.0",
        }
    }

    const goodsPercent = ratioToPercentTenths(goodsScaled, salesScaled)
    const totalCostPercent = ratioToPercentTenths(totalCostScaled, salesScaled)
    let extraPercent = "0.0"
    let marginPercent = "0.0"
    try {
        const diffExtra = subtractFixed(totalCostPercent, goodsPercent, {
            maxScale: 1,
            outputScale: 1,
        })
        extraPercent = compareDecimal(diffExtra, "0", 1) > 0 ? diffExtra : "0.0"
        const diffMargin = subtractFixed("100.0", totalCostPercent, {
            maxScale: 1,
            outputScale: 1,
        })
        marginPercent =
            compareDecimal(diffMargin, "0", 1) > 0 ? diffMargin : "0.0"
    } catch {
        extraPercent = "0.0"
        marginPercent = "0.0"
    }

    return {
        isLoss: false,
        lossRatioPercent: null,
        goodsPercent,
        extraPercent,
        marginPercent,
    }
}

function resolveCostMarginFromPercents(
    goodsCostPercent: string | null | undefined,
    extraChargesPercent: string | null | undefined,
    marginPercent: string | null | undefined,
    explicitLoss: boolean | undefined,
): ResolvedCostMarginModel | null {
    const hasAnyInput = Boolean(
        goodsCostPercent?.trim() ||
        extraChargesPercent?.trim() ||
        marginPercent?.trim() ||
        explicitLoss,
    )
    if (!hasAnyInput) return null

    const rawGoods = safeNonNegativePercent(goodsCostPercent) ?? "0.0"
    const rawExtra = safeNonNegativePercent(extraChargesPercent) ?? "0.0"

    let negativeMarginAbs: string | null = null
    let explicitLossMarginAbs: string | null = null
    if (marginPercent?.trim()) {
        try {
            const normalized = normalizeFixed(marginPercent, {
                maxScale: 4,
                outputScale: 1,
                allowNegative: true,
            })
            if (compareDecimal(normalized, "0", 1) < 0) {
                negativeMarginAbs = normalized.slice(1)
            } else if (explicitLoss && compareDecimal(normalized, "0", 1) > 0) {
                explicitLossMarginAbs = normalized
            }
        } catch {
            // 忽略非法百分比字符串
        }
    }

    let overflowLossAbs: string | null = null
    let combinedCost = "0.0"
    try {
        combinedCost = sumFixed([rawGoods, rawExtra], {
            maxScale: 1,
            outputScale: 1,
        })
        if (compareDecimal(combinedCost, "100.0", 1) > 0) {
            overflowLossAbs = subtractFixed(combinedCost, "100.0", {
                maxScale: 1,
                outputScale: 1,
            })
        }
    } catch {
        combinedCost = rawGoods
    }

    const loss = Boolean(explicitLoss || negativeMarginAbs || overflowLossAbs)
    if (loss) {
        return {
            isLoss: true,
            lossRatioPercent:
                negativeMarginAbs ?? overflowLossAbs ?? explicitLossMarginAbs,
            goodsPercent: "0.0",
            extraPercent: "0.0",
            marginPercent: "0.0",
        }
    }

    let resolvedMargin = "0.0"
    try {
        const remaining = subtractFixed("100.0", combinedCost, {
            maxScale: 1,
            outputScale: 1,
        })
        resolvedMargin =
            compareDecimal(remaining, "0", 1) > 0 ? remaining : "0.0"
    } catch {
        resolvedMargin = "0.0"
    }

    return {
        isLoss: false,
        lossRatioPercent: null,
        goodsPercent: rawGoods,
        extraPercent: rawExtra,
        marginPercent: resolvedMargin,
    }
}

/**
 * 成本-毛利结构条（docs/task-page-design-system.md §3.3.3）：
 * - 以销售总额为 `100%`，串联 `bg-primary/75`（货品采购成本）+ `bg-warning`（附加费用）+ `bg-success/75`（毛利空间）
 * - 整单亏损时整条转 `bg-destructive` 并自动展示溢亏比例徽章
 * - 全程基于 `BigInt` / `FixedDecimal` 计算占比，零 JS 浮点精度损失
 */
export function CostMarginBar({
    salesTotal,
    goodsCost,
    extraCharges,
    goodsCostPercent,
    extraChargesPercent,
    marginPercent,
    isLoss,
    lossLabel,
    label,
    summary,
    showLegend = false,
    height = "default",
    labels,
    ariaLabel = "成本与毛利结构",
    className,
    ...props
}: CostMarginBarProps) {
    const model = salesTotal?.trim()
        ? resolveCostMarginFromAmounts(
              salesTotal,
              goodsCost,
              extraCharges,
              isLoss,
              marginPercent,
          )
        : resolveCostMarginFromPercents(
              goodsCostPercent,
              extraChargesPercent,
              marginPercent,
              isLoss,
          )

    if (!model) {
        return null
    }

    const hasGoods = compareDecimal(model.goodsPercent, "0", 1) > 0
    const hasExtra = compareDecimal(model.extraPercent, "0", 1) > 0
    const hasMargin = compareDecimal(model.marginPercent, "0", 1) > 0

    const goodsLabel = labels?.goodsCost ?? "货品采购"
    const extraLabel = labels?.extraCharges ?? "附加费用"
    const marginLabel = labels?.margin ?? "毛利空间"
    const lossPrefix = labels?.loss ?? "整单亏损"

    const resolvedLossLabel =
        lossLabel ??
        (model.lossRatioPercent
            ? `${lossPrefix} -${model.lossRatioPercent}%`
            : lossPrefix)

    return (
        <div
            data-slot="cost-margin-bar"
            data-loss={model.isLoss ? "true" : "false"}
            className={cn("min-w-0 space-y-1.5", className)}
            {...props}
        >
            {label != null || summary != null ? (
                <div className="flex flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5 text-xs">
                    {label != null ? (
                        <span className="text-muted-foreground">{label}</span>
                    ) : (
                        <span />
                    )}
                    {summary != null ? (
                        <span className="num font-medium text-foreground">
                            {summary}
                        </span>
                    ) : null}
                </div>
            ) : null}

            <div className="flex items-center gap-2">
                <div
                    role="img"
                    aria-label={ariaLabel}
                    className={cn(
                        "flex min-w-0 flex-1 overflow-hidden rounded-full bg-muted",
                        barHeightClass[height],
                    )}
                >
                    {model.isLoss ? (
                        <div className="h-full w-full bg-destructive" />
                    ) : (
                        <>
                            {hasGoods ? (
                                <div
                                    title={`${goodsLabel}：${model.goodsPercent}%`}
                                    className="h-full bg-primary/75"
                                    style={{ width: `${model.goodsPercent}%` }}
                                />
                            ) : null}
                            {hasExtra ? (
                                <div
                                    title={`${extraLabel}：${model.extraPercent}%`}
                                    className="h-full bg-warning"
                                    style={{ width: `${model.extraPercent}%` }}
                                />
                            ) : null}
                            {hasMargin ? (
                                <div
                                    title={`${marginLabel}：${model.marginPercent}%`}
                                    className="h-full flex-1 bg-success/75"
                                />
                            ) : null}
                        </>
                    )}
                </div>

                {model.isLoss ? (
                    <span className="num inline-flex shrink-0 items-center gap-1 rounded-sm bg-destructive-soft px-1.5 py-0.5 text-tiny font-medium text-destructive-soft-foreground">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="size-3 shrink-0"
                        />
                        {resolvedLossLabel}
                    </span>
                ) : null}
            </div>

            {showLegend && !model.isLoss ? (
                <div className="num flex flex-wrap items-center gap-x-3 gap-y-1 text-tiny text-muted-foreground">
                    {hasGoods ? (
                        <span className="inline-flex items-center gap-1">
                            <span
                                aria-hidden="true"
                                className="size-1.5 shrink-0 rounded-full bg-primary/75"
                            />
                            <span>
                                {goodsLabel} {model.goodsPercent}%
                            </span>
                        </span>
                    ) : null}
                    {hasExtra ? (
                        <span className="inline-flex items-center gap-1 font-medium text-warning-soft-foreground">
                            <span
                                aria-hidden="true"
                                className="size-1.5 shrink-0 rounded-full bg-warning"
                            />
                            <span>
                                {extraLabel} {model.extraPercent}%
                            </span>
                        </span>
                    ) : null}
                    {hasMargin ? (
                        <span className="inline-flex items-center gap-1 font-medium text-success-soft-foreground">
                            <span
                                aria-hidden="true"
                                className="size-1.5 shrink-0 rounded-full bg-success/75"
                            />
                            <span>
                                {marginLabel} {model.marginPercent}%
                            </span>
                        </span>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}
