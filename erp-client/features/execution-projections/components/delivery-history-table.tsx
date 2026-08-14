"use client"

import { BusinessStatusBadge } from "@/components/business"
import type { ExecutionProjectionDelivery } from "@/features/execution-projections/types"

export function DeliveryHistoryTable({
    deliveries,
}: {
    deliveries: ExecutionProjectionDelivery[]
}) {
    return (
        <div className="overflow-x-auto rounded-xl border">
            <table className="w-full text-sm">
                <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                    <tr>
                        <th className="px-3 py-2">状态</th>
                        <th className="px-3 py-2">尝试</th>
                        <th className="px-3 py-2">最近</th>
                        <th className="px-3 py-2">下次</th>
                        <th className="px-3 py-2">确认</th>
                        <th className="px-3 py-2">摘要</th>
                    </tr>
                </thead>
                <tbody>
                    {deliveries.map((d) => (
                        <tr key={d.deliveryId} className="border-t">
                            <td className="px-3 py-2">
                                <BusinessStatusBadge
                                    context="list"
                                    label={d.statusLabel}
                                    tone={d.statusTone}
                                />
                            </td>
                            <td className="num px-3 py-2">{d.attemptCount}</td>
                            <td className="num px-3 py-2">
                                {d.lastAttemptAt ?? "—"}
                            </td>
                            <td className="num px-3 py-2">
                                {d.nextAttemptAt ?? "—"}
                            </td>
                            <td className="num px-3 py-2">
                                {d.mallAckAt ?? "—"}
                            </td>
                            <td className="px-3 py-2 text-xs text-muted-foreground">
                                {d.errorSummary ??
                                    d.mallExecutionBaseline ??
                                    "—"}
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    )
}
