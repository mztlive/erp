"use client"

import { DocumentSection } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    qtyWithUnit,
    type AcceptanceLineProgress,
    type AcceptanceOrderProgress,
} from "@/features/sales-orders/lib/acceptance-model"
import { FULFILLMENT_TYPE_LABEL } from "@/features/sales-orders/lib/acceptance-types"
import { cn } from "@/lib/utils"

export function AcceptanceProgressTable({
    progress,
    pendingHint,
    className,
}: {
    progress: AcceptanceOrderProgress
    pendingHint?: string
    className?: string
}) {
    const unit = progress.unitCode ?? ""
    const summary = progress.unitCode
        ? `已通过 ${qtyWithUnit(progress.acceptedQuantity, unit)} · 已交付 ${qtyWithUnit(progress.deliveredQuantity, unit)} · 销售 ${qtyWithUnit(progress.requiredQuantity, unit)}`
        : "按明细分别统计；不同单位不合并。"
    return (
        <DocumentSection
            className={className ?? "py-0"}
            title="验收进度"
            description={
                pendingHint ? (
                    <div className="flex flex-col gap-1">
                        <p>{summary}</p>
                        <p>{pendingHint}</p>
                    </div>
                ) : (
                    summary
                )
            }
        >
            {progress.lines.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    本单还没有销售明细。
                </p>
            ) : (
                <Table className="min-w-[32rem]">
                    <TableHeader>
                        <TableRow>
                            <TableHead>明细</TableHead>
                            <TableHead data-align="end">销售</TableHead>
                            <TableHead data-align="end">已交付</TableHead>
                            <TableHead data-align="end">已通过</TableHead>
                            <TableHead data-align="end">待验收</TableHead>
                            <TableHead className="w-52 max-w-52">
                                当前卡在
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {progress.lines.map((line) => (
                            <ProgressRow
                                key={line.salesOrderLineId}
                                line={line}
                            />
                        ))}
                    </TableBody>
                </Table>
            )}
        </DocumentSection>
    )
}

function ProgressRow({ line }: { line: AcceptanceLineProgress }) {
    const unit = line.unitCode
    return (
        <TableRow>
            <TableCell className="min-w-0 whitespace-normal break-keep">
                <div className="font-medium">
                    {line.lineNo} · {line.itemSnapshot}
                </div>
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.requiredQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.deliveredQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.acceptedQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.pendingQuantity, unit)}
            </TableCell>
            <TableCell className="w-52 max-w-52 min-w-0">
                <StuckCell line={line} />
            </TableCell>
        </TableRow>
    )
}

function StuckCell({ line }: { line: AcceptanceLineProgress }) {
    if (line.stuckKind === "accept" && line.pendingFacts.length === 1) {
        const fact = line.pendingFacts[0]
        if (!fact) {
            return (
                <span className="text-muted-foreground">{line.stuckLabel}</span>
            )
        }
        return (
            <div
                className="flex min-w-0 flex-col gap-0.5"
                title={line.stuckLabel}
            >
                <span>
                    {FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]}
                    {" · 待验 "}
                    {qtyWithUnit(fact.eligibleQuantity, line.unitCode)}
                </span>
                <span
                    className="num truncate text-muted-foreground"
                    title={fact.fulfillmentNo}
                >
                    {fact.fulfillmentNo}
                </span>
            </div>
        )
    }
    return (
        <span
            className={cn(
                "text-muted-foreground",
                line.stuckLabel.length > 24 && "line-clamp-2 break-keep",
            )}
            title={line.stuckLabel}
        >
            {line.stuckLabel}
        </span>
    )
}
