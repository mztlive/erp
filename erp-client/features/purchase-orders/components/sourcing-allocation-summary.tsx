import {
    CheckCircle2Icon,
    ScissorsIcon,
    SplitIcon,
    TriangleAlertIcon,
    ZapIcon,
} from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import {
    isPositiveQuantity,
    type GlobalAddressState,
} from "../lib/sourcing/address-grouping"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { summarizeRowPricing } from "../lib/sourcing/pricing"
import { quantityUnits } from "../lib/sourcing/quantity"
import { sourceAccentForIndex } from "../lib/sourcing/source-palette"

type BarSegment = {
    key: string
    label: string
    qty: string
    widthPercent: string
    barClass: string
}

/**
 * 顶部全局多源配平条：
 * - 分色堆叠进度条直观呈现每个来源（现有库存 / 集采入仓 / 代发直发 / 集采直发）的占比与超配溢出；
 * - 当存在混合履约时显式展示「总需求 = 仓库/库存统筹 + 供应商直发」守恒等式；
 * - 提供一键配平动作（直发自动缩减多余地址 / 直发一键补配余量 / 剩余缺口拆新源）。
 */
export function SourcingAllocationSummary({
    row,
    addressState,
    onTrimDirectExcess,
    onFillDirectShortfall,
    onSplitResidual,
    canSplitNewSource = false,
}: {
    row: SourcingEditorRow
    addressState?: GlobalAddressState | null
    onTrimDirectExcess?: () => void
    onFillDirectShortfall?: () => void
    onSplitResidual?: () => void
    canSplitNewSource?: boolean
}) {
    const included = row.selected || row.partiallySelected
    const idPrefix = `sourcing-balance-${toAutomationIdSegment(row.product.salesOrderLineId)}`

    let excess = false
    let complete = false
    let diffText = "—"
    let demandUnits = BigInt(0)
    let allocatedUnits = BigInt(0)

    try {
        demandUnits = quantityUnits(row.product.remainingQuantity)
        allocatedUnits = included ? quantityUnits(row.quantity) : BigInt(0)
        const cmp = compareDecimal(
            included ? row.quantity : "0",
            row.product.remainingQuantity,
            6,
        )
        excess = included && cmp > 0
        complete = included && cmp === 0
        diffText = compactFixed(
            subtractFixed(
                excess ? row.quantity : row.product.remainingQuantity,
                excess
                    ? row.product.remainingQuantity
                    : included
                      ? row.quantity
                      : "0",
                { maxScale: 6, outputScale: 6 },
            ),
        )
    } catch {
        // 未完成数量输入时不把剩余需求伪装成零
    }

    const partial = included && !excess && !complete && diffText !== "—"

    // 构造各已启用来源的分色堆叠片段（全定点 BigInt 计算宽度百分比，零浮点误差）
    const denomUnits =
        allocatedUnits > demandUnits ? allocatedUnits : demandUnits
    const segments: BarSegment[] = []
    if (included && denomUnits > BigInt(0)) {
        row.allocations.forEach(({ line }, idx) => {
            if (!line.selected || !isPositiveQuantity(line.quantity || "0"))
                return
            try {
                const qty = compactFixed(line.quantity.trim())
                const units = quantityUnits(qty)
                if (units <= BigInt(0)) return
                const option = findSourcingOption(row.product, line.basisId)
                const route = option ? sourcingRouteLabel(option) : "待选"
                const basisPctTimes10 = (units * BigInt(1000)) / denomUnits
                const whole = basisPctTimes10 / BigInt(10)
                const tenths = basisPctTimes10 % BigInt(10)
                const widthPercent = `${whole.toString()}.${tenths.toString()}%`
                segments.push({
                    key: line.rowKey,
                    label: `源${idx + 1} ${route}`,
                    qty,
                    widthPercent,
                    barClass: sourceAccentForIndex(idx).barClass,
                })
            } catch {
                // 跳过非法值
            }
        })
    }

    // 价格与毛利紧凑摘要
    const pricingSummary = summarizeRowPricing(row)
    const pricingText = (() => {
        const activeCount = row.allocations.filter(
            ({ line }) => line.selected,
        ).length
        const base =
            row.allocations.length > 1
                ? `${activeCount || row.allocations.length} 家组合`
                : "单源供货"
        if (pricingSummary.kind === "stock") return `${base} · 库存核算`
        if (
            pricingSummary.kind === "purchase" &&
            pricingSummary.unitCostGross
        ) {
            const marginText = pricingSummary.hasInvertedSource
                ? "价格倒挂"
                : pricingSummary.margin?.marginRateText
                  ? `毛利 ${pricingSummary.margin.marginRateText}`
                  : ""
            return [base, `¥${pricingSummary.unitCostGross}`, marginText]
                .filter(Boolean)
                .join(" · ")
        }
        if (pricingSummary.kind === "multiple") {
            return pricingSummary.hasInvertedSource
                ? `${base} · 多价格 · 含倒挂`
                : `${base} · 多价格`
        }
        return `${base} · 待选方案`
    })()

    const showMixedEquation = Boolean(
        included &&
        addressState &&
        addressState.hasNonDirectSources &&
        addressState.directSources.some((src) => src.selected),
    )

    const hasDirectOverTarget = Boolean(
        addressState &&
        isPositiveQuantity(addressState.directOverTargetQty) &&
        onTrimDirectExcess,
    )

    const hasDirectShortfall = Boolean(
        addressState &&
        !excess &&
        isPositiveQuantity(addressState.directShortfallQty) &&
        addressState.directSources.some((src) => src.selected) &&
        onFillDirectShortfall,
    )

    return (
        <div
            className={cn(
                "shrink-0 border-b border-border/70 px-4 py-2.5 sm:px-6",
                excess
                    ? "bg-destructive-soft/45"
                    : complete
                      ? "bg-success-soft/35"
                      : "bg-surface-sunken",
            )}
            aria-label="商品分配进度"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
                {/* 左侧：配平数字与混合模式守恒等式 */}
                <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1 text-xs">
                    <span className="inline-flex items-baseline gap-1">
                        <span className="text-muted-foreground">已分配</span>
                        <span
                            className={cn(
                                "text-body-compact font-semibold",
                                excess
                                    ? "text-destructive"
                                    : complete
                                      ? "text-success-soft-foreground"
                                      : "text-foreground",
                            )}
                        >
                            <QuantityValue
                                value={included ? row.quantity : "0"}
                                unit=""
                            />
                        </span>
                        <span className="text-muted-foreground">/</span>
                        <QuantityValue
                            value={row.product.remainingQuantity}
                            unit={row.product.unit}
                            className="font-medium text-foreground"
                        />
                    </span>

                    {excess ? (
                        <span className="num inline-flex items-center gap-1 rounded-sm bg-destructive-soft px-1.5 py-0.5 text-tiny font-semibold text-destructive-soft-foreground">
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-3"
                            />
                            超出 {diffText} {row.product.unit}
                        </span>
                    ) : complete ? (
                        <span className="inline-flex items-center gap-1 rounded-sm bg-success-soft px-1.5 py-0.5 text-tiny font-medium text-success-soft-foreground">
                            <CheckCircle2Icon
                                aria-hidden="true"
                                className="size-3"
                            />
                            已配平
                        </span>
                    ) : partial ? (
                        <span className="num inline-flex items-center gap-1 rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny font-medium text-warning-soft-foreground">
                            尚缺 {diffText} {row.product.unit}
                        </span>
                    ) : !included ? (
                        <span className="rounded-sm bg-muted px-1.5 py-0.5 text-tiny text-muted-foreground">
                            已暂缓
                        </span>
                    ) : null}

                    {showMixedEquation && addressState ? (
                        <>
                            <span
                                aria-hidden="true"
                                className="hidden text-border sm:inline"
                            >
                                |
                            </span>
                            <span className="text-muted-foreground">
                                仓库/库存统筹{" "}
                                <strong className="num font-semibold text-foreground">
                                    {addressState.nonDirectTotalQty}{" "}
                                    {row.product.unit}
                                </strong>
                                {" + "}供应商直发{" "}
                                <strong
                                    className={cn(
                                        "num font-semibold",
                                        isPositiveQuantity(
                                            addressState.directOverTargetQty,
                                        )
                                            ? "text-destructive"
                                            : "text-foreground",
                                    )}
                                >
                                    {addressState.directAssignedTotalQty}{" "}
                                    {row.product.unit}
                                </strong>
                                <span className="num text-muted-foreground">
                                    （直发额度 {addressState.directTargetQty}{" "}
                                    {row.product.unit}）
                                </span>
                            </span>
                        </>
                    ) : null}
                </div>

                {/* 右侧：一键智能配平动作 + 综合报价摘要 */}
                <div className="flex flex-wrap items-center gap-2">
                    {hasDirectOverTarget && addressState ? (
                        <Button
                            id={`${idPrefix}-trim-direct`}
                            type="button"
                            size="xs"
                            variant="destructive"
                            onClick={onTrimDirectExcess}
                        >
                            <ScissorsIcon aria-hidden="true" />
                            直发自动缩减至 {addressState.directTargetQty}{" "}
                            {row.product.unit}
                        </Button>
                    ) : null}

                    {hasDirectShortfall && addressState ? (
                        <Button
                            id={`${idPrefix}-fill-direct`}
                            type="button"
                            size="xs"
                            variant="secondary"
                            onClick={onFillDirectShortfall}
                        >
                            <ZapIcon aria-hidden="true" />
                            直发补配 +{addressState.directShortfallQty}{" "}
                            {row.product.unit}
                        </Button>
                    ) : null}

                    {partial && canSplitNewSource && onSplitResidual ? (
                        <Button
                            id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split-residual`}
                            type="button"
                            size="xs"
                            variant="outline"
                            className="bg-card"
                            onClick={onSplitResidual}
                        >
                            <SplitIcon aria-hidden="true" />
                            余量 {diffText} {row.product.unit} 拆给新来源
                        </Button>
                    ) : null}

                    <span
                        className={cn(
                            "hidden text-xs md:inline",
                            pricingSummary.hasInvertedSource
                                ? "font-medium text-destructive"
                                : "text-muted-foreground",
                        )}
                    >
                        {pricingText}
                    </span>
                </div>
            </div>

            {/* 分色堆叠配平条 + 多源图例 */}
            <div className="mt-2 flex items-center gap-3">
                <div
                    className="relative flex h-2 flex-1 overflow-hidden rounded-full bg-border/75"
                    aria-hidden="true"
                >
                    {segments.map((seg) => (
                        <div
                            key={seg.key}
                            title={`${seg.label}：${seg.qty} ${row.product.unit}`}
                            className={cn(
                                "h-full border-r border-card/80 transition-[width] duration-200 last:border-r-0",
                                excess ? "bg-destructive/80" : seg.barClass,
                            )}
                            style={{ width: seg.widthPercent }}
                        />
                    ))}
                </div>
                {segments.length > 1 ? (
                    <div className="hidden shrink-0 flex-wrap items-center gap-2.5 text-tiny text-muted-foreground lg:flex">
                        {segments.map((seg) => (
                            <span
                                key={seg.key}
                                className="inline-flex items-center gap-1"
                            >
                                <span
                                    aria-hidden="true"
                                    className={cn(
                                        "size-2 rounded-full",
                                        seg.barClass,
                                    )}
                                />
                                <span>{seg.label}</span>
                                <span className="num font-medium text-foreground">
                                    {seg.qty}
                                </span>
                            </span>
                        ))}
                    </div>
                ) : null}
            </div>
        </div>
    )
}
