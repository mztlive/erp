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

export function AcceptanceProgressTable({
    progress,
}: {
    progress: AcceptanceOrderProgress
}) {
    const unit = progress.unitCode ?? ""
    return (
        <DocumentSection
            className="py-0"
            title="验收进度"
            description={
                progress.unitCode
                    ? `已通过 ${qtyWithUnit(progress.acceptedQuantity, unit)} · 已交付 ${qtyWithUnit(progress.deliveredQuantity, unit)} · 销售 ${qtyWithUnit(progress.requiredQuantity, unit)}`
                    : "按明细分别统计；不同单位不合并。"
            }
        >
            {progress.lines.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    本单还没有销售明细。
                </p>
            ) : (
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>明细</TableHead>
                            <TableHead data-align="end">销售</TableHead>
                            <TableHead data-align="end">已交付</TableHead>
                            <TableHead data-align="end">已通过</TableHead>
                            <TableHead data-align="end">待验收</TableHead>
                            <TableHead>当前卡在</TableHead>
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
            <TableCell className="whitespace-normal">
                <div className="font-medium">
                    {line.lineNo} · {line.itemSnapshot}
                </div>
            </TableCell>
            <TableCell data-align="end" className="num">
                {qtyWithUnit(line.requiredQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num">
                {qtyWithUnit(line.deliveredQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num">
                {qtyWithUnit(line.acceptedQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num">
                {qtyWithUnit(line.pendingQuantity, unit)}
            </TableCell>
            <TableCell className="whitespace-normal text-muted-foreground">
                {line.stuckLabel}
            </TableCell>
        </TableRow>
    )
}
