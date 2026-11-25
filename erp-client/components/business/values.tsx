"use client"

import * as React from "react"
import { CircleAlertIcon, type LucideIcon } from "lucide-react"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import { describeMoney, type MoneyMagnitude } from "@/lib/money-display"

type BusinessStatusContext = "list" | "detail" | "preview"

interface BusinessStatusBadgeProps extends Omit<
    React.ComponentProps<typeof StatusBadge>,
    "children"
> {
    /** 状态的业务解释；列表中用提示展示，详情与预览中直接展示。 */
    description?: React.ReactNode
    /** 只控制描述信息的密度，不推导或改变状态。 */
    context?: BusinessStatusContext
}

/**
 * 全站通用状态枚举兜底中文映射（唯一数据源，供状态徽章与导入/批处理结果共用）：
 * 调用方误把枚举原值（PENDING 等）当 label 传入时，替换成业务词，禁止原值上屏。
 * 仅命中「全大写 + 下划线」形态的原值 token，中文标签不受影响。
 */
const STATUS_LABEL_FALLBACK: Readonly<Record<string, string>> = {
    PENDING: "待处理",
    IN_PROGRESS: "处理中",
    PROCESSING: "处理中",
    COMPLETED: "已完成",
    SUCCEEDED: "已完成",
    SKIPPED: "已跳过",
    FAILED: "处理失败",
    REJECTED: "未通过",
    BLOCKED: "暂不可操作",
    ACCEPTED: "已受理",
    TRANSFERRED: "已转交",
    UNKNOWN: "结果未知",
    RESULT_UNKNOWN: "结果未知",
    CANCELLED: "已取消",
    CANCELED: "已取消",
    CLOSED: "已关闭",
    VOID: "已作废",
    DRAFT: "草稿",
    EXPIRED: "已过期",
    EXCEPTION: "异常",
}

const rawStatusTokenPattern = /^[A-Z][A-Z0-9_]*$/

function resolveStatusLabel(label: string): string {
    return rawStatusTokenPattern.test(label)
        ? (STATUS_LABEL_FALLBACK[label] ?? label)
        : label
}

/**
 * 为现有 StatusBadge 补充展示上下文和可选说明。
 *
 * 组件不包含领域状态机；调用方仍需明确传入 label、tone 与可选 icon。
 */
function BusinessStatusBadge({
    description,
    context = "list",
    label,
    ...props
}: BusinessStatusBadgeProps) {
    const badge = (
        <StatusBadge
            data-context={context}
            label={resolveStatusLabel(label)}
            {...props}
        />
    )

    if (description == null) {
        return badge
    }

    if (context === "list") {
        return (
            <TooltipProvider>
                <Tooltip>
                    <TooltipTrigger
                        render={
                            <span className="inline-flex cursor-help rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring" />
                        }
                    >
                        {badge}
                        <span className="sr-only">{description}</span>
                    </TooltipTrigger>
                    <TooltipContent>{description}</TooltipContent>
                </Tooltip>
            </TooltipProvider>
        )
    }

    return (
        <span
            data-slot="business-status"
            data-context={context}
            className={cn(
                "min-w-0",
                context === "preview"
                    ? "flex flex-col items-start gap-1"
                    : "inline-flex flex-wrap items-center gap-2",
            )}
        >
            {badge}
            <span className="text-xs text-muted-foreground">{description}</span>
        </span>
    )
}

type BusinessStatus = Readonly<{
    label: string
    tone?: StatusTone
    icon?: LucideIcon
    description?: React.ReactNode
}>

type StatusTrack = Readonly<{
    id: React.Key
    label: React.ReactNode
    status: BusinessStatus
}>

type StatusTrackSummaryVariant = "inline" | "stacked" | "table"

interface StatusTrackSummaryProps extends Omit<
    React.ComponentProps<"dl">,
    "children"
> {
    tracks: readonly StatusTrack[]
    variant?: StatusTrackSummaryVariant
    appearance?: "badge" | "text" | "quiet"
}

/** 并列展示互不覆盖的状态轴，例如主状态、履约、回款和开票。 */
function StatusTrackSummary({
    tracks,
    variant = "inline",
    appearance = "badge",
    className,
    "aria-label": ariaLabel = "业务状态",
    ...props
}: StatusTrackSummaryProps) {
    const statusContext: BusinessStatusContext =
        variant === "inline"
            ? "list"
            : variant === "stacked"
              ? "preview"
              : "detail"

    return (
        <dl
            data-slot="status-track-summary"
            data-variant={variant}
            aria-label={ariaLabel}
            className={cn(
                variant === "inline" &&
                    "flex flex-wrap items-center gap-x-4 gap-y-2",
                variant === "stacked" && "flex flex-col divide-y divide-border",
                variant === "table" &&
                    "grid gap-3 sm:grid-cols-2 lg:grid-cols-3",
                className,
            )}
            {...props}
        >
            {tracks.map((track) => (
                <div
                    key={track.id}
                    className={cn(
                        "min-w-0",
                        variant === "inline" && "flex items-center gap-2",
                        variant === "stacked" &&
                            "flex items-start justify-between gap-4 py-3 first:pt-0 last:pb-0",
                        variant === "table" &&
                            "flex flex-col items-start gap-2 rounded-lg border border-border bg-card p-3",
                    )}
                >
                    <dt className="text-xs font-medium text-muted-foreground">
                        {track.label}
                    </dt>
                    <dd className="min-w-0">
                        <BusinessStatusBadge
                            context={statusContext}
                            appearance={appearance}
                            label={track.status.label}
                            tone={track.status.tone}
                            icon={track.status.icon}
                            description={track.status.description}
                        />
                    </dd>
                </div>
            ))}
        </dl>
    )
}

type ParsedDecimal = Readonly<{
    negative: boolean
    integer: string
    fraction: string
}>

const decimalPattern = /^([+-]?)(\d+)(?:\.(\d+))?$/

const integerFormatter = new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: 0,
    useGrouping: true,
})

const cnyIntegerFormatter = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    currencyDisplay: "symbol",
    minimumFractionDigits: 0,
    maximumFractionDigits: 0,
})

const decimalSeparator =
    new Intl.NumberFormat("zh-CN", {
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
    })
        .formatToParts(0)
        .find((part) => part.type === "decimal")?.value ?? "."

function parseDecimal(value: string): ParsedDecimal | null {
    const match = decimalPattern.exec(value.trim())

    if (!match) {
        return null
    }

    const integer = match[2].replace(/^0+(?=\d)/, "")
    const fraction = match[3] ?? ""
    const isZero = integer === "0" && !/[1-9]/.test(fraction)

    return {
        negative: match[1] === "-" && !isZero,
        integer,
        fraction,
    }
}

function withPrecision(
    value: ParsedDecimal,
    precision: number | undefined,
): ParsedDecimal {
    if (precision === undefined) {
        return value
    }

    if (!Number.isSafeInteger(precision) || precision < 0) {
        throw new RangeError("precision 必须是非负安全整数")
    }

    const keptFraction = value.fraction.slice(0, precision)
    const paddedFraction = keptFraction.padEnd(precision, "0")
    const nextDigit = value.fraction.charAt(precision)

    if (nextDigit === "" || nextDigit < "5") {
        const isZero = value.integer === "0" && !/[1-9]/.test(paddedFraction)

        return {
            negative: value.negative && !isZero,
            integer: value.integer,
            fraction: paddedFraction,
        }
    }

    const magnitude = `${value.integer}${paddedFraction}`
    const minimumLength = value.integer.length + precision
    const incremented = (BigInt(magnitude || "0") + BigInt(1))
        .toString()
        .padStart(minimumLength, "0")
    const integer =
        precision === 0 ? incremented : incremented.slice(0, -precision)
    const fraction = precision === 0 ? "" : incremented.slice(-precision)
    const isZero = integer === "0" && !/[1-9]/.test(fraction)

    return {
        negative: value.negative && !isZero,
        integer,
        fraction,
    }
}

function signedInteger(value: ParsedDecimal) {
    return BigInt(`${value.negative ? "-" : ""}${value.integer}`)
}

function insertFraction(parts: Intl.NumberFormatPart[], fraction: string) {
    if (fraction === "") {
        return parts.map((part) => part.value).join("")
    }

    let insertAfter = -1

    parts.forEach((part, index) => {
        if (part.type === "integer" || part.type === "group") {
            insertAfter = index
        }
    })

    const before = parts.slice(0, insertAfter + 1).map((part) => part.value)
    const after = parts.slice(insertAfter + 1).map((part) => part.value)

    return [...before, decimalSeparator, fraction, ...after].join("")
}

function formatDecimal(
    rawValue: string,
    precision?: number,
    style: "number" | "cny" = "number",
) {
    const parsed = parseDecimal(rawValue)

    if (!parsed) {
        return rawValue
    }

    const displayValue = withPrecision(parsed, precision)
    const formatter = style === "cny" ? cnyIntegerFormatter : integerFormatter
    const parts = formatter.formatToParts(signedInteger(displayValue))

    return insertFraction(parts, displayValue.fraction)
}

type TaxBasis = "gross" | "net"

/**
 * 税口径由标签表达，含税汇总保留字重强调；颜色统一用于金额量级。
 */
const TAX_AMOUNT_LABEL_TONES: Readonly<Record<string, string>> = {
    含税金额: "font-semibold",
    含税合计: "font-semibold",
    含税: "font-semibold",
    行含税: "font-semibold",
    含税小计: "font-semibold",
    不含税金额: "",
    不含税: "",
    税额: "",
}

/** 按展示标签取税额三角的配色类；不认识的标签返回空串（保持默认色）。 */
export function taxAmountToneClass(label: unknown): string {
    return typeof label === "string"
        ? (TAX_AMOUNT_LABEL_TONES[label.trim()] ?? "")
        : ""
}

type MoneyValueSize = "inherit" | "compact" | "section" | "summary" | "hero"
type MoneyValueDisplay = "full" | "compact" | "exact"
type MoneyValueEmphasis = "subtle" | "default" | "strong"

const moneyMagnitudeClasses: Record<MoneyMagnitude, string> = {
    base: "text-foreground",
    wan: "text-money-wan",
    million: "text-money-million",
    yi: "text-money-yi font-semibold",
}

const moneyValueSizeClassNames: Record<MoneyValueSize, string> = {
    inherit: "",
    compact: "text-value-compact font-semibold tracking-tight",
    section: "text-value-section font-semibold tracking-tight",
    summary: "text-value-summary font-semibold tracking-tight",
    hero: "text-value-hero font-semibold tracking-tight",
}

const moneyValueEmphasisClassNames: Record<MoneyValueEmphasis, string> = {
    subtle: "font-normal",
    default: "font-medium",
    strong: "font-semibold",
}

interface MoneyValueProps extends Omit<
    React.ComponentProps<"span">,
    "children"
> {
    value?: string | null
    taxBasis?: TaxBasis
    unavailableReason?: React.ReactNode
    /** 只调整金额数字；税口径徽章与不可用原因保持辅助字号。 */
    size?: MoneyValueSize
    /** 行内金额字重层级：subtle=弱化参考价，default=默认常规，strong=重点强调。 */
    emphasis?: MoneyValueEmphasis
    /** full：精确金额附量级；compact：简写附精确金额；exact：仅精确金额。 */
    display?: MoneyValueDisplay
    /** 隐藏辅助金额行；compact 模式仍向辅助技术提供完整金额。 */
    showHint?: boolean
    /** 继承父级语义色（如负毛利、超预估警示等）；默认保持沉稳墨色 text-foreground。 */
    inheritColor?: boolean
}

/** 精确展示 CNY 十进制字符串；仅格式化，不执行金额计算。 */
function MoneyValue({
    value,
    taxBasis,
    unavailableReason,
    size = "inherit",
    emphasis = "default",
    display = "full",
    showHint = true,
    inheritColor = false,
    className,
    onCopy,
    ...props
}: MoneyValueProps) {
    const amount = unavailableReason == null ? describeMoney(value) : null
    const isUnavailable = amount == null
    const compactPrimary = display === "compact" && amount?.compact != null
    const hint = display === "exact" ? null : amount?.compact

    return (
        <span
            data-slot="money-value"
            data-tax-basis={taxBasis}
            data-unavailable={isUnavailable || undefined}
            data-size={size}
            data-emphasis={emphasis}
            data-display={display}
            data-magnitude={amount?.magnitude}
            className={cn(
                "inline-flex min-w-0 flex-nowrap items-baseline gap-2",
                moneyValueEmphasisClassNames[emphasis],
                className,
            )}
            {...props}
            onCopy={(event) => {
                onCopy?.(event)
                const selection =
                    event.currentTarget.ownerDocument.getSelection()
                if (
                    event.defaultPrevented ||
                    !amount ||
                    !selection ||
                    selection.isCollapsed ||
                    !event.currentTarget.contains(selection.anchorNode) ||
                    !event.currentTarget.contains(selection.focusNode)
                )
                    return
                event.clipboardData.setData("text/plain", amount.copyValue)
                event.preventDefault()
            }}
        >
            <span className="inline-flex min-w-0 flex-col">
                <span
                    data-slot="money-value-number"
                    aria-hidden={compactPrimary || undefined}
                    className={cn(
                        "num whitespace-nowrap",
                        moneyValueSizeClassNames[size],
                        compactPrimary && "print:hidden",
                        compactPrimary &&
                            !inheritColor &&
                            moneyMagnitudeClasses[amount.magnitude],
                        compactPrimary &&
                            inheritColor &&
                            amount.magnitude === "yi" &&
                            "font-semibold",
                        !compactPrimary && !inheritColor && "text-foreground",
                        isUnavailable && "text-muted-foreground",
                    )}
                >
                    {amount
                        ? compactPrimary
                            ? amount.compact
                            : amount.exact
                        : "—"}
                </span>
                {hint && amount && showHint ? (
                    <span
                        data-slot="money-value-hint"
                        aria-hidden={!compactPrimary || undefined}
                        className={cn(
                            "num whitespace-nowrap text-xs font-normal",
                            compactPrimary
                                ? "text-muted-foreground print:text-inherit"
                                : `${moneyMagnitudeClasses[amount.magnitude]} print:hidden`,
                        )}
                    >
                        {compactPrimary ? amount.exact : hint}
                    </span>
                ) : null}
                {!showHint && compactPrimary && amount ? (
                    <span className="sr-only print:not-sr-only">
                        {amount.exact}
                    </span>
                ) : null}
            </span>
            {taxBasis ? (
                <Badge variant="neutral">
                    {taxBasis === "gross" ? "含税" : "不含税"}
                </Badge>
            ) : null}
            {unavailableReason != null ? (
                <span className="text-xs text-muted-foreground">
                    {unavailableReason}
                </span>
            ) : null}
        </span>
    )
}

type ValueTone =
    | "neutral"
    | "muted"
    | "info"
    | "success"
    | "warning"
    | "destructive"

const valueToneClassNames: Record<ValueTone, string> = {
    neutral: "text-foreground",
    muted: "text-muted-foreground",
    info: "text-info-soft-foreground",
    success: "text-success-soft-foreground",
    warning: "text-warning-soft-foreground",
    destructive: "text-destructive-soft-foreground",
}

interface QuantityValueProps extends Omit<
    React.ComponentProps<"span">,
    "children"
> {
    value: string
    unit: React.ReactNode
    tone?: ValueTone
    /** 继承父级语义色；默认按 tone 着色。 */
    inheritColor?: boolean
}

/** 精确展示数量十进制字符串及其单位，不进行单位换算。 */
function QuantityValue({
    value,
    unit,
    tone = "neutral",
    inheritColor = false,
    className,
    ...props
}: QuantityValueProps) {
    return (
        <span
            data-slot="quantity-value"
            className={cn("inline-flex items-baseline gap-1", className)}
            {...props}
        >
            <span
                className={cn(
                    "num",
                    !inheritColor && valueToneClassNames[tone],
                )}
            >
                {formatDecimal(value)}
            </span>
            <span className="text-xs text-muted-foreground">{unit}</span>
        </span>
    )
}

interface RateValueProps extends Omit<
    React.ComponentProps<"span">,
    "children"
> {
    /** 已按百分点表达的十进制字符串；组件不会乘以 100。 */
    value: string
    precision: number
    tone?: ValueTone
    /** 继承父级语义色；默认按 tone 着色。 */
    inheritColor?: boolean
}

/** 按指定精度展示百分点十进制字符串；不会把值转换为浮点数。 */
function RateValue({
    value,
    precision,
    tone = "neutral",
    inheritColor = false,
    className,
    ...props
}: RateValueProps) {
    return (
        <span
            data-slot="rate-value"
            className={cn(
                "num whitespace-nowrap",
                !inheritColor && valueToneClassNames[tone],
                className,
            )}
            {...props}
        >
            {formatDecimal(value, precision)}%
        </span>
    )
}

type DocumentTotalItem = Readonly<{
    id: React.Key
    label: React.ReactNode
    value: React.ReactNode
    basis?: React.ReactNode
    warning?: React.ReactNode
}>

interface DocumentTotalsProps extends Omit<
    React.ComponentProps<"section">,
    "children" | "title"
> {
    title?: React.ReactNode
    items: readonly DocumentTotalItem[]
    warning?: React.ReactNode
}

/** 展示单据汇总值、口径与警告；所有数值均由调用方显式提供。 */
function DocumentTotals({
    title = "汇总",
    items,
    warning,
    className,
    ...props
}: DocumentTotalsProps) {
    return (
        <section
            data-slot="document-totals"
            className={cn("flex flex-col gap-3", className)}
            {...props}
        >
            {title != null ? (
                <h2 className="font-heading text-section-title font-semibold tracking-tight text-foreground">
                    {title}
                </h2>
            ) : null}

            <dl className="divide-y divide-border rounded-lg border border-border bg-card">
                {items.map((item) => (
                    <div
                        key={item.id}
                        className="flex flex-col gap-2 p-3 sm:flex-row sm:items-start sm:justify-between"
                    >
                        <dt className="flex min-w-0 flex-wrap items-center gap-2 text-xs font-medium text-muted-foreground">
                            <span>{item.label}</span>
                            {item.basis != null ? (
                                <Badge variant="neutral">{item.basis}</Badge>
                            ) : null}
                        </dt>
                        <dd className="min-w-0 text-body-compact sm:text-right">
                            <div
                                className={cn(
                                    "num font-medium text-foreground",
                                    taxAmountToneClass(item.label),
                                )}
                            >
                                {item.value}
                            </div>
                            {item.warning != null ? (
                                <div className="mt-1 flex items-start gap-1 text-xs text-warning-soft-foreground">
                                    <CircleAlertIcon
                                        aria-hidden="true"
                                        className="mt-0.5 size-3 shrink-0"
                                    />
                                    <span>{item.warning}</span>
                                </div>
                            ) : null}
                        </dd>
                    </div>
                ))}
            </dl>

            {warning != null ? (
                <Alert variant="warning">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertDescription>{warning}</AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}

export {
    BusinessStatusBadge,
    DocumentTotals,
    MoneyValue,
    QuantityValue,
    RateValue,
    STATUS_LABEL_FALLBACK,
    StatusTrackSummary,
    resolveStatusLabel,
    type BusinessStatus,
    type BusinessStatusBadgeProps,
    type BusinessStatusContext,
    type DocumentTotalItem,
    type DocumentTotalsProps,
    type MoneyValueProps,
    type MoneyValueEmphasis,
    type MoneyValueSize,
    type MoneyValueDisplay,
    type QuantityValueProps,
    type RateValueProps,
    type StatusTrack,
    type StatusTrackSummaryProps,
    type StatusTrackSummaryVariant,
    type TaxBasis,
    type ValueTone,
}
