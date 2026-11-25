import {
    CheckCircle2Icon,
    ScissorsIcon,
    TriangleAlertIcon,
    ZapIcon,
} from "lucide-react"
import { QuantityValue, sourceAccentForIndex } from "@/components/business"
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
import { quantityUnits } from "../lib/sourcing/quantity"

type BarSegment = {
    key: string
    label: string
    qty: string
    widthPercent: string
    barClass: string
}

/**
 * 左侧栏顶部配平总览卡：
 * - 单一数据源呈现「已分配 / 总需求」、分色堆叠进度条与各来源数量图例；
 * - 混合履约直接表达「直发 X + 仓库/库存 Y」，去除晦涩的“直发额度”术语；
 * - 仅在失衡时就地提供一键配平动作。
 */
export function SourcingAllocationSummary({
    row,
    addressState,
    onTrimDirectExcess,
    onFillDirectShortfall,
}: {
    row: SourcingEditorRow
    addressState?: GlobalAddressState | null
    onTrimDirectExcess?: () => void
    onFillDirectShortfall?: () => void
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
                segments.push({
                    key: line.rowKey,
                    label: `源${idx + 1} ${route}`,
                    qty,
                    widthPercent: `${whole.toString()}.${tenths.toString()}%`,
                    barClass: sourceAccentForIndex(idx).barClass,
                })
            } catch {
                // 跳过非法值
            }
        })
    }

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

    const hasQuickAction = hasDirectOverTarget || hasDirectShortfall

    return (
        <div
            className={cn(
                "shrink-0 rounded-lg border bg-card px-3.5 py-2.5 shadow-2xs",
                excess ? "border-destructive-border" : "border-border/75",
            )}
            aria-label="商品分配进度"
        >
            <div className="flex items-center justify-between gap-2">
                <div className="flex items-baseline gap-1.5">
                    <span className="text-xs text-muted-foreground">
                        已分配
                    </span>
                    <span
                        className={cn(
                            "text-value-compact font-semibold tracking-tight",
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
                    <span className="text-xs text-muted-foreground">/</span>
                    <QuantityValue
                        value={row.product.remainingQuantity}
                        unit={row.product.unit}
                        className="text-body-compact font-medium text-muted-foreground"
                    />
                </div>

                {excess ? (
                    <span className="num inline-flex items-center gap-1 rounded-sm bg-destructive-soft px-2 py-0.5 text-xs font-semibold text-destructive-soft-foreground">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="size-3"
                        />
                        超出 {diffText} {row.product.unit}
                    </span>
                ) : complete ? (
                    <span className="inline-flex items-center gap-1 rounded-sm bg-success-soft px-2 py-0.5 text-xs font-medium text-success-soft-foreground">
                        <CheckCircle2Icon
                            aria-hidden="true"
                            className="size-3"
                        />
                        已配平
                    </span>
                ) : partial ? (
                    <span className="num inline-flex items-center gap-1 rounded-sm bg-warning-soft px-2 py-0.5 text-xs font-medium text-warning-soft-foreground">
                        尚缺 {diffText} {row.product.unit}
                    </span>
                ) : !included ? (
                    <span className="rounded-sm bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                        已暂缓
                    </span>
                ) : null}
            </div>

            {/* 分色堆叠配平条 */}
            <div
                className="mt-2 flex h-1.5 w-full overflow-hidden rounded-full bg-muted"
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

            {/* 来源图例 */}
            {segments.length > 1 ? (
                <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-tiny text-muted-foreground">
                    {segments.map((seg) => (
                        <span
                            key={seg.key}
                            className="inline-flex items-center gap-1"
                        >
                            <span
                                aria-hidden="true"
                                className={cn(
                                    "size-1.5 rounded-full",
                                    seg.barClass,
                                )}
                            />
                            <span>{seg.label}</span>
                            <span className="num font-medium text-foreground">
                                {seg.qty} {row.product.unit}
                            </span>
                        </span>
                    ))}
                </div>
            ) : null}

            {/* 失衡时就地提供一键配平动作 */}
            {hasQuickAction ? (
                <div className="mt-2 flex flex-wrap items-center gap-1.5 border-t border-border/50 pt-2">
                    {hasDirectOverTarget && addressState ? (
                        <Button
                            id={`${idPrefix}-trim-direct`}
                            type="button"
                            size="xs"
                            variant="destructive"
                            onClick={onTrimDirectExcess}
                        >
                            <ScissorsIcon aria-hidden="true" />
                            自动缩减多余直发 -{
                                addressState.directOverTargetQty
                            }{" "}
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
                            直发自动补配 +{addressState.directShortfallQty}{" "}
                            {row.product.unit}
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}
