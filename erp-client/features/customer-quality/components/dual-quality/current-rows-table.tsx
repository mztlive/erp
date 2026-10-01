"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CurrentQualityRow } from "../../dual-types"
import type { PatchDual } from "../../lib/dual-filter-state"

export function CurrentRowsTable({
    items,
    dimension,
    patchDual,
}: {
    items: readonly CurrentQualityRow[]
    dimension: string
    patchDual: PatchDual
}) {
    const grouped = dimension !== "customer"
    return (
        <div className="min-w-0 overflow-x-auto rounded-xl border border-border">
            <table className="w-full min-w-[560px] border-collapse text-sm">
                <thead>
                    <tr className="border-b border-border text-left text-xs text-muted-foreground">
                        <th className="px-3 py-2 font-medium">
                            {grouped ? "分组" : "客户"}
                        </th>
                        <th className="px-3 py-2 font-medium">现任负责人</th>
                        <th className="px-3 py-2 font-medium">现任组织</th>
                        {grouped ? (
                            <th className="px-3 py-2 text-right font-medium">
                                客户数
                            </th>
                        ) : null}
                        <th className="px-3 py-2 text-right font-medium">
                            订单数
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            含税总额
                        </th>
                        <th className="px-3 py-2 text-right font-medium">
                            缺版本
                        </th>
                        {grouped ? (
                            <th className="px-3 py-2 text-right font-medium">
                                下钻
                            </th>
                        ) : null}
                    </tr>
                </thead>
                <tbody>
                    {items.map((row) => {
                        const drill =
                            dimension === "owner_user" && row.groupId != null
                                ? `user:${row.groupId}`
                                : dimension === "owner_org" &&
                                    row.groupId != null
                                  ? `org:${row.groupId}`
                                  : null
                        return (
                            <tr
                                key={row.rowId}
                                className="border-b border-border last:border-0"
                            >
                                <td className="max-w-48 px-3 py-2">
                                    <div className="truncate font-medium">
                                        {grouped
                                            ? (row.label ?? row.rowId)
                                            : (row.customerName ?? row.rowId)}
                                    </div>
                                    {!grouped && row.customerNo ? (
                                        <div className="num truncate text-xs text-muted-foreground">
                                            {row.customerNo}
                                        </div>
                                    ) : null}
                                </td>
                                <td className="max-w-40 truncate px-3 py-2 text-[13px]">
                                    {row.ownerUserName ??
                                        row.ownerUserId ??
                                        "—"}
                                </td>
                                <td className="max-w-40 truncate px-3 py-2 text-[13px]">
                                    {row.ownerOrgUnitName ??
                                        row.ownerOrgUnitId ??
                                        "—"}
                                </td>
                                {grouped ? (
                                    <td className="num px-3 py-2 text-right">
                                        {row.customerCount ?? "—"}
                                    </td>
                                ) : null}
                                <td className="num px-3 py-2 text-right">
                                    {row.orderCount}
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
                                {grouped ? (
                                    <td className="px-3 py-2 text-right">
                                        {drill ? (
                                            <Button
                                                id={`customers-quality-dual-drill-${toAutomationIdSegment(row.rowId)}`}
                                                type="button"
                                                variant="link"
                                                size="xs"
                                                onClick={() =>
                                                    patchDual({
                                                        ownerGroup: drill,
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
                                ) : null}
                            </tr>
                        )
                    })}
                </tbody>
            </table>
        </div>
    )
}
