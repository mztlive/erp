import { CalendarClockIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { quantityUnits } from "../lib/sourcing/quantity"

/** 汇总只计算参与分配的来源；超分配单独显示，非法数量保持未知。 */
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
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-6 gap-y-2 border-b border-border/60 bg-surface-sunken px-5 py-2.5">
            <dl className="flex flex-wrap items-center gap-x-5 gap-y-1.5">
                <div className="flex items-baseline gap-2">
                    <dt className="text-xs text-muted-foreground">需求</dt>
                    <dd className="text-value-compact font-semibold tracking-tight">
                        <QuantityValue
                            value={row.product.remainingQuantity}
                            unit={row.product.unit}
                        />
                    </dd>
                </div>
                <span
                    aria-hidden="true"
                    className="hidden h-4 w-px bg-border/80 sm:block"
                />
                <div className="flex items-baseline gap-2">
                    <dt className="text-xs text-muted-foreground">已分配</dt>
                    <dd className="text-value-compact font-semibold tracking-tight">
                        <QuantityValue
                            value={row.quantity}
                            unit={row.product.unit}
                        />
                    </dd>
                </div>
                <span
                    aria-hidden="true"
                    className="hidden h-4 w-px bg-border/80 sm:block"
                />
                <div
                    className={cn(
                        "flex items-baseline gap-2",
                        excess && "text-destructive",
                        partial && "text-warning-soft-foreground",
                    )}
                >
                    <dt
                        className={cn(
                            "text-xs",
                            !excess && !partial && "text-muted-foreground",
                        )}
                    >
                        {excess ? "超出需求" : "待分配"}
                    </dt>
                    <dd className="text-value-compact font-semibold tracking-tight">
                        <QuantityValue
                            value={remaining}
                            unit={row.product.unit}
                            tone={partial ? "warning" : "neutral"}
                            className={cn(
                                excess && "[&_.num]:text-destructive",
                                complete && "[&_.num]:text-success",
                            )}
                        />
                    </dd>
                </div>
            </dl>

            <div className="flex min-w-0 flex-1 items-center justify-end gap-5">
                {percentText !== null ? (
                    <div
                        className="hidden min-w-28 max-w-48 flex-1 items-center gap-2.5 md:flex"
                        aria-hidden="true"
                    >
                        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-border/80">
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
                                "num shrink-0 text-xs font-medium",
                                excess
                                    ? "text-destructive"
                                    : complete
                                      ? "text-success-soft-foreground"
                                      : "text-muted-foreground",
                            )}
                        >
                            {excess ? "超出" : `${percentText}%`}
                        </span>
                    </div>
                ) : null}

                <div className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                    <CalendarClockIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0 text-muted-foreground/80"
                    />
                    <span>最晚交付</span>
                    <span className="num font-medium text-foreground">
                        {row.product.deliveryDeadline || "未限定"}
                    </span>
                </div>
            </div>
        </div>
    )
}
