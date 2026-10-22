"use client"

import { QuantityValue } from "@/components/business"
import { TableToolbar } from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SupplierQuoteSource } from "../lib/supplier-quote-export"

export function SupplierQuoteSelection({
    source,
    lineIds,
    supplierIds,
    disabled,
    onLinesChange,
    onSuppliersChange,
}: {
    source: SupplierQuoteSource
    lineIds: string[]
    supplierIds: string[]
    disabled: boolean
    onLinesChange: (ids: string[]) => void
    onSuppliersChange: (ids: string[]) => void
}) {
    const groups = source.groups
        .map((group) => ({
            ...group,
            lines: group.lines.filter((line) => lineIds.includes(line.lineId)),
        }))
        .filter((group) => group.lines.length)
    const toggle = (ids: string[], id: string, checked: boolean) =>
        checked
            ? [...new Set([...ids, id])]
            : ids.filter((value) => value !== id)
    return (
        <div className="grid min-h-0 items-start gap-4 lg:grid-cols-2">
            <section
                className="min-w-0 overflow-hidden rounded-lg border bg-card"
                aria-label="询价商品选择"
            >
                <TableToolbar
                    className="border-b bg-muted/30 px-3 py-2"
                    actions={
                        <>
                            <Button
                                id="supplier-quote-lines-all"
                                type="button"
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                className="px-2"
                                onClick={() =>
                                    onLinesChange(
                                        source.lines.map((line) => line.lineId),
                                    )
                                }
                            >
                                全选
                            </Button>
                            <Button
                                id="supplier-quote-lines-clear"
                                type="button"
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                className="px-2 text-muted-foreground"
                                onClick={() => onLinesChange([])}
                            >
                                清空
                            </Button>
                        </>
                    }
                >
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                        <span className="font-medium">询价商品</span>
                        <span className="text-xs text-muted-foreground">
                            已选 {lineIds.length} / {source.lines.length}
                        </span>
                    </div>
                </TableToolbar>
                <div className="max-h-80 divide-y overflow-y-auto">
                    {source.lines.map((line) => {
                        const id = `supplier-quote-line-${toAutomationIdSegment(line.lineId)}`
                        const count = source.groups.filter((group) =>
                            group.lines.some(
                                (item) => item.lineId === line.lineId,
                            ),
                        ).length
                        return (
                            <div
                                key={line.lineId}
                                className="flex items-start gap-3 px-3 py-3 transition-colors hover:bg-muted/30"
                            >
                                <Checkbox
                                    id={id}
                                    checked={lineIds.includes(line.lineId)}
                                    disabled={disabled}
                                    className="mt-0.5 shrink-0"
                                    onCheckedChange={(checked) =>
                                        onLinesChange(
                                            toggle(
                                                lineIds,
                                                line.lineId,
                                                Boolean(checked),
                                            ),
                                        )
                                    }
                                />
                                <div className="min-w-0 space-y-1">
                                    <Label
                                        htmlFor={id}
                                        className="block leading-5 wrap-anywhere"
                                    >
                                        {line.name}
                                    </Label>
                                    <p className="text-xs text-muted-foreground">
                                        {line.specification || line.skuCode} ·{" "}
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={line.unit}
                                        />{" "}
                                        · {count} 家供应商
                                    </p>
                                    {(line.issue || !count) && (
                                        <p className="text-xs text-destructive">
                                            {line.issue ||
                                                "未找到供给关系，请补齐或取消勾选"}
                                        </p>
                                    )}
                                </div>
                            </div>
                        )
                    })}
                </div>
            </section>
            <section
                className="min-w-0 overflow-hidden rounded-lg border bg-card"
                aria-label="报价供应商选择"
            >
                <TableToolbar
                    className="border-b bg-muted/30 px-3 py-2"
                    actions={
                        <>
                            <Button
                                id="supplier-quote-suppliers-all"
                                type="button"
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                className="px-2"
                                onClick={() =>
                                    onSuppliersChange(
                                        source.groups.map(
                                            (group) => group.supplierId,
                                        ),
                                    )
                                }
                            >
                                全选
                            </Button>
                            <Button
                                id="supplier-quote-suppliers-clear"
                                type="button"
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                className="px-2 text-muted-foreground"
                                onClick={() => onSuppliersChange([])}
                            >
                                清空
                            </Button>
                        </>
                    }
                >
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                        <span className="font-medium">供应商</span>
                        <span className="text-xs text-muted-foreground">
                            已选{" "}
                            {
                                groups.filter((group) =>
                                    supplierIds.includes(group.supplierId),
                                ).length
                            }{" "}
                            / {groups.length}
                        </span>
                    </div>
                </TableToolbar>
                <div className="max-h-80 divide-y overflow-y-auto">
                    {!groups.length && (
                        <p className="p-3 text-sm text-muted-foreground">
                            请先选择有供给关系的商品。
                        </p>
                    )}
                    {groups.map((group) => {
                        const id = `supplier-quote-supplier-${toAutomationIdSegment(group.supplierId)}`
                        const hasSuspended = group.lines.some((line) =>
                            line.statuses.some((status) => status !== "ACTIVE"),
                        )
                        return (
                            <div
                                key={group.supplierId}
                                className="flex items-start gap-3 px-3 py-3 transition-colors hover:bg-muted/30"
                            >
                                <Checkbox
                                    id={id}
                                    checked={supplierIds.includes(
                                        group.supplierId,
                                    )}
                                    disabled={disabled}
                                    className="mt-0.5 shrink-0"
                                    onCheckedChange={(checked) =>
                                        onSuppliersChange(
                                            toggle(
                                                supplierIds,
                                                group.supplierId,
                                                Boolean(checked),
                                            ),
                                        )
                                    }
                                />
                                <div className="min-w-0 space-y-1">
                                    <Label
                                        htmlFor={id}
                                        className="block leading-5 wrap-anywhere"
                                    >
                                        {group.supplierName}
                                    </Label>
                                    <p className="text-xs text-muted-foreground">
                                        {group.supplierNo
                                            ? `${group.supplierNo} · `
                                            : ""}
                                        {group.lines.length} 条商品需求
                                    </p>
                                    {hasSuspended && (
                                        <p className="text-xs text-muted-foreground">
                                            含暂停或停止的供给关系，请核对本次是否询价。
                                        </p>
                                    )}
                                </div>
                            </div>
                        )
                    })}
                </div>
            </section>
        </div>
    )
}
