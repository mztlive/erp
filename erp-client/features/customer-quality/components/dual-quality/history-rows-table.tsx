"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { HistoryQualityRow } from "../../dual-types"
import type { PatchDual } from "../../lib/dual-filter-state"

export function HistoryRowsTable({
    items,
    dimension,
    patchDual,
}: {
    items: readonly HistoryQualityRow[]
    dimension: string
    patchDual: PatchDual
}) {
    return (
        <div className="min-w-0 overflow-x-auto rounded-xl border border-border">
            <table className="w-full min-w-[560px] border-collapse text-sm">
                <thead>
                    <tr className="border-b border-border text-left text-xs text-muted-foreground">
                        <th className="px-3 py-2 font-medium">
                            {dimension === "attribution_user"
                                ? "历史归属销售"
                                : "历史归属组织"}
                        </th>
                        <th className="px-3 py-2 font-medium">归属客户</th>
                        <th className="px-3 py-2 text-right font-medium">
                            订单数
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            含税总额
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            缺版本
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            下钻
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {items.map((row) => {
                        const drill =
                            row.groupId != null
                                ? dimension === "attribution_user"
                                    ? `attribution_user:${row.groupId}`
                                    : `attribution_org:${row.groupId}`
                                : null
                        const identity =
                            dimension === "attribution_user"
                                ? (row.attributionUserName ??
                                  row.attributionUserId ??
                                  row.label ??
                                  row.rowId)
                                : (row.attributionOrgUnitName ??
                                  row.attributionOrgUnitId ??
                                  row.label ??
                                  row.rowId)
                        return (
                            <tr
                                key={row.rowId}
                                className="border-b border-border last:border-0"
                            >
                                <td className="max-w-48 px-3 py-2">
                                    <div className="truncate font-medium">
                                        {identity}
                                    </div>
                                    {row.orderNo ? (
                                        <div className="num truncate text-xs text-muted-foreground">
                                            {row.orderNo}
                                        </div>
                                    ) : null}
                                </td>
                                <td className="max-w-40 truncate px-3 py-2 text-[13px]">
                                    {row.customerName ?? row.customerId ?? "—"}
                                </td>
                                <td className="num px-3 py-2 text-right">
                                    {row.orderCount ?? "—"}
                                </td>
                                <td className="px-3 py-2 text-right">
                                    <MoneyValue
                                        value={row.grossTotal}
                                        taxBasis="gross"
                                    />
                                </td>
                                <td className="num px-3 py-2 text-right">
                                    {row.unpricedCount}
                                </td>
                                <td className="px-3 py-2 text-right">
                                    {drill ? (
                                        <Button
                                            id={`customers-quality-dual-drill-${toAutomationIdSegment(row.rowId)}`}
                                            type="button"
                                            variant="link"
                                            size="xs"
                                            onClick={() =>
                                                patchDual({
                                                    attributionGroup: drill,
                                                    scopeVersion: null,
                                                    dualPage: null,
                                                })
                                            }
                                        >
                                            下钻
                                        </Button>
                                    ) : (
                                        "—"
                                    )}
                                </td>
                            </tr>
                        )
                    })}
                </tbody>
            </table>
        </div>
    )
}
