"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
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
        <div className="min-w-0">
            <Table data-density="comfortable" className="min-w-[560px]">
                <TableHeader>
                    <TableRow>
                        <TableHead>
                            {dimension === "attribution_user"
                                ? "历史归属销售"
                                : "历史归属组织"}
                        </TableHead>
                        <TableHead>归属客户</TableHead>
                        <TableHead data-align="end">订单数</TableHead>
                        <TableHead data-align="end">含税总额</TableHead>
                        <TableHead data-align="end">缺版本</TableHead>
                        <TableHead data-align="end">下钻</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
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
                            <TableRow key={row.rowId}>
                                <TableCell className="max-w-48">
                                    <div className="truncate font-medium">
                                        {identity}
                                    </div>
                                    {row.orderNo ? (
                                        <div className="num truncate text-xs text-muted-foreground">
                                            {row.orderNo}
                                        </div>
                                    ) : null}
                                </TableCell>
                                <TableCell className="max-w-40 truncate">
                                    {row.customerName ?? row.customerId ?? "—"}
                                </TableCell>
                                <TableCell className="num" data-align="end">
                                    {row.orderCount ?? "—"}
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={row.grossTotal}
                                        taxBasis="gross"
                                    />
                                </TableCell>
                                <TableCell className="num" data-align="end">
                                    {row.unpricedCount}
                                </TableCell>
                                <TableCell data-align="end">
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
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}
