import { QuantityValue } from "@/components/business"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { quantityUnits } from "../lib/sourcing/quantity"

/** 页头紧凑分配进度胶囊：一处说清「已分 / 需求 / 缺口或超出」，避免在弹窗内反复重复数字。 */
export function SourcingAllocationSummary({ row }: { row: SourcingEditorRow }) {
    const included = row.selected || row.partiallySelected
    let excess = false
    let complete = false
    let remaining = "—"
    let percentText: string | null = null

    try {
        const cmp = compareDecimal(
            row.quantity,
            row.product.remainingQuantity,
            6,
        )
        excess = cmp > 0
        complete = included && cmp === 0
        remaining = compactFixed(
            subtractFixed(
                excess ? row.quantity : row.product.remainingQuantity,
                excess ? row.product.remainingQuantity : row.quantity,
                { maxScale: 6, outputScale: 6 },
            ),
        )
        const demandUnits = quantityUnits(row.product.remainingQuantity)
        const allocatedUnits = quantityUnits(row.quantity)
        if (demandUnits > BigInt(0)) {
            const raw = (allocatedUnits * BigInt(100)) / demandUnits
            const clamped =
                raw < BigInt(0)
                    ? BigInt(0)
                    : raw > BigInt(100)
                      ? BigInt(100)
                      : raw
            percentText = clamped.toString()
        }
    } catch {
        // 未完成数量输入时不把剩余需求伪装成零。
    }

    const partial = included && !excess && !complete && remaining !== "—"

    return (
        <div
            className="flex shrink-0 items-center gap-3 rounded-lg border border-border/70 bg-surface-sunken px-3 py-1.5"
            aria-label="商品分配进度"
        >
            <div className="flex items-baseline gap-1.5 text-xs">
                <span className="text-muted-foreground">已分配</span>
                <span className="text-body-compact font-semibold text-foreground">
                    <QuantityValue
                        value={included ? row.quantity : "0"}
                        unit=""
                    />
                </span>
                <span className="text-muted-foreground">/</span>
                <QuantityValue
                    value={row.product.remainingQuantity}
                    unit={row.product.unit}
                    className="text-xs [&_.num]:text-muted-foreground"
                />
            </div>

            {percentText !== null ? (
                <div
                    className="hidden items-center gap-2 sm:flex"
                    aria-hidden="true"
                >
                    <div className="h-1.5 w-16 overflow-hidden rounded-full bg-border/80">
                        <div
                            className={cn(
                                "h-full rounded-full transition-[width,background-color] duration-200",
                                excess
                                    ? "bg-destructive"
                                    : complete
                                      ? "bg-success"
                                      : included
                                        ? "bg-primary"
                                        : "bg-muted-foreground/35",
                            )}
                            style={{ width: `${percentText}%` }}
                        />
                    </div>
                    <span
                        className={cn(
                            "num text-xs font-medium",
                            excess
                                ? "text-destructive"
                                : complete
                                  ? "text-success-soft-foreground"
                                  : partial
                                    ? "text-warning-soft-foreground"
                                    : "text-muted-foreground",
                        )}
                    >
                        {excess
                            ? `超出 ${remaining}`
                            : partial
                              ? `剩 ${remaining}`
                              : `${percentText}%`}
                    </span>
                </div>
            ) : null}
        </div>
    )
}
