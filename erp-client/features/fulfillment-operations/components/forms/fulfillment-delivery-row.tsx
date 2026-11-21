"use client"

import { Fragment } from "react"
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { TableCell, TableRow } from "@/components/ui/table"
import type { DeliveryTrackingEntry, FulfillmentDraft } from "../../types"
import { FulfillmentTrackingEntriesField } from "./fulfillment-tracking-entries-field"

export type DeliveryRowData = {
    salesOrderLineId: string
    title: string
    sku: string
    quantity: string
    unit: string
    packages: DeliveryTrackingEntry[]
    pending: boolean
    linePrefix: string
}

/** 商品摘要和常驻编辑区，筛选与折叠仅改变可见性，不丢弃未添加输入。 */
export function FulfillmentDeliveryRow({
    row,
    visible,
    expanded,
    selected,
    disabled,
    draft,
    onChange,
    onSelect,
    onOpen,
    onClose,
    onNext,
}: {
    row: DeliveryRowData
    visible: boolean
    expanded: boolean
    selected: boolean
    disabled?: boolean
    draft: Extract<
        FulfillmentDraft,
        { type: "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" }
    >
    onChange: (draft: FulfillmentDraft) => void
    onSelect: (selected: boolean) => void
    onOpen: () => void
    onClose: () => void
    onNext?: () => void
}) {
    const { linePrefix, salesOrderLineId, packages } = row
    return (
        <Fragment>
            <TableRow
                hidden={!visible}
                data-state={selected ? "selected" : undefined}
            >
                <TableCell className="px-3">
                    <Checkbox
                        id={`${linePrefix}-select`}
                        aria-label={`选择${row.title} ${row.sku}`}
                        checked={selected}
                        disabled={disabled}
                        onCheckedChange={onSelect}
                    />
                </TableCell>
                <TableCell className="whitespace-normal">
                    <p
                        className="line-clamp-2 break-words text-body-compact font-medium"
                        title={row.title}
                    >
                        {row.title}
                    </p>
                    <p className="mt-1 break-words text-xs text-muted-foreground">
                        {row.sku ? `${row.sku} · ` : ""}本次发货{" "}
                        <span className="num">
                            {row.quantity}
                            {row.unit}
                        </span>
                    </p>
                </TableCell>
                <TableCell className="min-w-0">
                    {packages.length ? (
                        <div className="flex min-w-0 items-center gap-2">
                            <span
                                className="num truncate text-xs"
                                title={`${packages[0].carrier || "未填写物流公司"} ${packages[0].trackingNo}`}
                            >
                                {packages[0].carrier || "未填写物流公司"}{" "}
                                {packages[0].trackingNo}
                            </span>
                            {packages.length > 1 ? (
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    另 {packages.length - 1} 个
                                </span>
                            ) : null}
                        </div>
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            未登记
                        </span>
                    )}
                    {row.pending ? (
                        <p className="mt-1 text-xs text-destructive">
                            输入尚未添加
                        </p>
                    ) : null}
                </TableCell>
                <TableCell className="text-right">
                    <Button
                        id={`${linePrefix}-toggle`}
                        type="button"
                        size="sm"
                        variant="ghost"
                        aria-label={`${expanded ? "收起" : packages.length ? "编辑" : "填写"}${row.title}的物流`}
                        aria-expanded={expanded}
                        aria-controls={`${linePrefix}-editor`}
                        onClick={expanded ? onClose : onOpen}
                    >
                        {expanded
                            ? "收起"
                            : packages.length
                              ? "编辑"
                              : "填写物流"}
                        {expanded ? (
                            <ChevronUpIcon data-icon="inline-end" />
                        ) : (
                            <ChevronDownIcon data-icon="inline-end" />
                        )}
                    </Button>
                </TableCell>
            </TableRow>
            <TableRow
                hidden={!visible || !expanded}
                className="bg-muted/20 hover:bg-muted/20"
            >
                <TableCell colSpan={4} className="whitespace-normal p-3">
                    <div
                        id={`${linePrefix}-editor`}
                        className="space-y-3"
                        aria-label={`${row.title}的物流录入`}
                    >
                        <FulfillmentTrackingEntriesField
                            id={linePrefix}
                            salesOrderLineId={salesOrderLineId}
                            entries={draft.trackingEntries ?? []}
                            pendingInput={row.pending}
                            disabled={disabled}
                            onChange={(trackingEntries, inputAdded) =>
                                onChange({
                                    ...draft,
                                    trackingEntries,
                                    pendingTrackingLineIds: inputAdded
                                        ? draft.pendingTrackingLineIds?.filter(
                                              (id) => id !== salesOrderLineId,
                                          )
                                        : draft.pendingTrackingLineIds,
                                })
                            }
                            onPendingInputChange={(pending) =>
                                onChange({
                                    ...draft,
                                    pendingTrackingLineIds: pending
                                        ? [
                                              ...new Set([
                                                  ...(draft.pendingTrackingLineIds ??
                                                      []),
                                                  salesOrderLineId,
                                              ]),
                                          ]
                                        : draft.pendingTrackingLineIds?.filter(
                                              (id) => id !== salesOrderLineId,
                                          ),
                                })
                            }
                        />
                        <div className="flex justify-end gap-2">
                            <Button
                                id={`${linePrefix}-close`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={onClose}
                            >
                                收起
                            </Button>
                            {onNext ? (
                                <Button
                                    id={`${linePrefix}-next`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={onNext}
                                >
                                    下一项待登记
                                </Button>
                            ) : null}
                        </div>
                    </div>
                </TableCell>
            </TableRow>
        </Fragment>
    )
}
