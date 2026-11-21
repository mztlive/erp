"use client"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { useArrangement } from "../hooks/queries"

/** 展示当前履约收货安排，与销售审核版本中的原始要求分开。 */
export function ArrangementSummary({ salesOrderId }: { salesOrderId: string }) {
    const query = useArrangement("sales-orders", salesOrderId, true)
    if (query.isPending)
        return (
            <DetailWorkspaceSection width="form" title="当前收货安排">
                <p className="text-xs text-muted-foreground">
                    正在读取当前收货安排…
                </p>
            </DetailWorkspaceSection>
        )
    if (query.isError)
        return (
            <DetailWorkspaceSection width="form" title="当前收货安排">
                <div className="space-y-2.5">
                    <p className="text-body-compact text-muted-foreground">
                        当前收货安排暂不可用：{query.error.message}
                    </p>
                    <Button
                        id={`sales-arrangement-${toAutomationIdSegment(salesOrderId)}-retry`}
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => void query.refetch()}
                    >
                        重新读取
                    </Button>
                </div>
            </DetailWorkspaceSection>
        )
    if (!query.data.lines.length) return null
    return (
        <DetailWorkspaceSection
            width="form"
            title="当前收货安排"
            subtitle="通过页面上方「收货安排」补充或调整未发部分，已发包裹保持原地址。"
        >
            <div className="divide-y divide-border/60">
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
                            className="space-y-1.5 py-3.5 text-body-compact first:pt-0 last:pb-0"
                        >
                            <p className="font-medium text-foreground">
                                {line.item_name}{" "}
                                <span className="num font-normal text-muted-foreground">
                                    · {compactFixed(line.quantity)} {line.unit}
                                </span>
                            </p>
                            {line.allocations.map((row) => (
                                <p
                                    key={`${row.recipient}-${row.phone}-${row.region_code}-${row.address}`}
                                    className="text-muted-foreground"
                                >
                                    {row.recipient} ·{" "}
                                    <span className="num">{row.phone}</span> ·{" "}
                                    {row.region_name}
                                    {row.address} ·{" "}
                                    <span className="num">
                                        {compactFixed(row.quantity)} {line.unit}
                                    </span>
                                </p>
                            ))}
                            {pending !== "0.000000" ? (
                                <p className="num text-xs text-warning-soft-foreground">
                                    待补收货地址：{compactFixed(pending)}{" "}
                                    {line.unit}
                                </p>
                            ) : null}
                        </div>
                    )
                })}
            </div>
        </DetailWorkspaceSection>
    )
}
