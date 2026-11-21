import { QuantityValue } from "@/components/business"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"

/** 汇总只计算参与分配的来源；超分配单独显示，非法数量保持未知。 */
export function SourcingAllocationSummary({ row }: { row: SourcingEditorRow }) {
    let excess = false
    let remaining = "—"
    try {
        excess =
            compareDecimal(row.quantity, row.product.remainingQuantity, 6) > 0
        remaining = compactFixed(
            subtractFixed(
                excess ? row.quantity : row.product.remainingQuantity,
                excess ? row.product.remainingQuantity : row.quantity,
                { maxScale: 6, outputScale: 6 },
            ),
        )
    } catch {
        // 未完成数量输入时不把剩余需求伪装成零。
    }
    return (
        <dl className="grid shrink-0 grid-cols-2 gap-x-6 gap-y-2 border-b border-border px-5 py-3 sm:grid-cols-4">
            <div>
                <dt className="text-xs text-muted-foreground">需求</dt>
                <dd className="num mt-1 text-value-compact font-semibold tracking-tight">
                    <QuantityValue
                        value={row.product.remainingQuantity}
                        unit={row.product.unit}
                    />
                </dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">已分配</dt>
                <dd className="num mt-1 text-value-compact font-semibold tracking-tight">
                    <QuantityValue
                        value={row.quantity}
                        unit={row.product.unit}
                    />
                </dd>
            </div>
            <div className={cn(excess && "text-destructive")}>
                <dt
                    className={cn(
                        "text-xs",
                        !excess && "text-muted-foreground",
                    )}
                >
                    {excess ? "超出需求" : "待分配"}
                </dt>
                <dd className="num mt-1 text-value-compact font-semibold tracking-tight">
                    <QuantityValue value={remaining} unit={row.product.unit} />
                </dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">最晚交付日期</dt>
                <dd className="num mt-1 text-body-compact font-medium">
                    {row.product.deliveryDeadline || "未限定"}
                </dd>
            </div>
        </dl>
    )
}
