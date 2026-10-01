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
        <div className="min-w-0">
            <Table data-density="comfortable" className="min-w-[560px]">
                <TableHeader>
                    <TableRow>
                        <TableHead>{grouped ? "分组" : "客户"}</TableHead>
                        <TableHead>现任负责人</TableHead>
                        <TableHead>现任组织</TableHead>
                        {grouped ? (
                            <TableHead data-align="end">客户数</TableHead>
                        ) : null}
                        <TableHead data-align="end">订单数</TableHead>
                        <TableHead data-align="end">含税总额</TableHead>
                        <TableHead data-align="end">缺版本</TableHead>
                        {grouped ? (
                            <TableHead data-align="end">下钻</TableHead>
                        ) : null}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {items.map((row) => {
                        const drill =
                            dimension === "owner_user" && row.groupId != null
                                ? `user:${row.groupId}`
                                : dimension === "owner_org" &&
                                    row.groupId != null
                                  ? `org:${row.groupId}`
                                  : null
                        return (
                            <TableRow key={row.rowId}>
                                <TableCell className="max-w-48">
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
                                </TableCell>
                                <TableCell className="max-w-40 truncate">
                                    {row.ownerUserName ??
                                        row.ownerUserId ??
                                        "—"}
                                </TableCell>
                                <TableCell className="max-w-40 truncate">
                                    {row.ownerOrgUnitName ??
                                        row.ownerOrgUnitId ??
                                        "—"}
                                </TableCell>
                                {grouped ? (
                                    <TableCell className="num" data-align="end">
                                        {row.customerCount ?? "—"}
                                    </TableCell>
                                ) : null}
                                <TableCell className="num" data-align="end">
                                    {row.orderCount}
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
                                {grouped ? (
                                    <TableCell data-align="end">
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
                                    </TableCell>
                                ) : null}
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}
