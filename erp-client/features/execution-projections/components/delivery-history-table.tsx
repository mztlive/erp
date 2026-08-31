"use client"

import { BusinessStatusBadge } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { ExecutionProjectionDelivery } from "@/features/execution-projections/types"

export function DeliveryHistoryTable({
    deliveries,
}: {
    deliveries: ExecutionProjectionDelivery[]
}) {
    return (
        <div className="overflow-hidden rounded-lg border">
            <Table data-density="compact">
                <TableHeader>
                    <TableRow>
                        <TableHead>状态</TableHead>
                        <TableHead>尝试</TableHead>
                        <TableHead>最近</TableHead>
                        <TableHead>下次</TableHead>
                        <TableHead>确认</TableHead>
                        <TableHead>摘要</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {deliveries.map((d) => (
                        <TableRow key={d.deliveryId}>
                            <TableCell>
                                <BusinessStatusBadge
                                    context="list"
                                    label={d.statusLabel}
                                    tone={d.statusTone}
                                />
                            </TableCell>
                            <TableCell className="num">
                                {d.attemptCount}
                            </TableCell>
                            <TableCell className="num">
                                {d.lastAttemptAt ?? "—"}
                            </TableCell>
                            <TableCell className="num">
                                {d.nextAttemptAt ?? "—"}
                            </TableCell>
                            <TableCell className="num">
                                {d.mallAckAt ?? "—"}
                            </TableCell>
                            <TableCell className="max-w-sm whitespace-normal text-xs text-muted-foreground">
                                {d.errorSummary ??
                                    d.mallExecutionBaseline ??
                                    "—"}
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}
