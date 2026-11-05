"use client"
import { Button } from "@/components/ui/button"
import { compactFixed, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useArrangement } from "../hooks/queries"

/** 展示当前履约收货安排，与销售审核版本中的原始要求分开。 */
export function ArrangementSummary({ salesOrderId }: { salesOrderId: string }) {
    const query = useArrangement("sales-orders", salesOrderId, true)
    if (query.isPending)
        return (
            <p className="text-sm text-muted-foreground">
                正在读取当前收货安排…
            </p>
        )
    if (query.isError)
        return (
            <section className="space-y-2 rounded-lg border p-4">
                <p className="text-sm">
                    当前收货安排暂不可用：{query.error.message}
                </p>
                <Button
                    id={`sales-arrangement-${toAutomationIdSegment(salesOrderId)}-retry`}
                    variant="outline"
                    size="sm"
                    onClick={() => void query.refetch()}
                >
                    重新读取
                </Button>
            </section>
        )
    if (!query.data.lines.length) return null
    return (
        <section className="space-y-3 rounded-lg border border-border/70 p-4 md:p-5">
            <div>
                <h2 className="text-lg font-semibold">当前收货安排</h2>
                <p className="text-xs text-muted-foreground">
                    通过页面上方「收货安排」补充或调整未发部分，已发包裹保持原地址。
                </p>
            </div>
            {query.data.lines.map((line) => {
                const pending = subtractFixed(
                    line.quantity,
                    sumFixed(
                        line.allocations.map((row) => row.quantity),
                        { maxScale: 6, outputScale: 6 },
                    ),
                    { maxScale: 6, outputScale: 6 },
                )
                return (
                    <div
                        key={line.line_id}
                        className="space-y-1.5 border-t pt-3 text-sm"
                    >
                        <p className="font-medium">
                            {line.item_name} · {compactFixed(line.quantity)}{" "}
                            {line.unit}
                        </p>
                        {line.allocations.map((row) => (
                            <p
                                key={`${row.recipient}-${row.phone}-${row.region_code}-${row.address}`}
                                className="text-muted-foreground"
                            >
                                {row.recipient} · {row.phone} ·{" "}
                                {row.region_name}
                                {row.address} · {compactFixed(row.quantity)}{" "}
                                {line.unit}
                            </p>
                        ))}
                        {pending !== "0.000000" ? (
                            <p className="text-muted-foreground">
                                待补收货地址：{compactFixed(pending)}{" "}
                                {line.unit}
                            </p>
                        ) : null}
                    </div>
                )
            })}
        </section>
    )
}
