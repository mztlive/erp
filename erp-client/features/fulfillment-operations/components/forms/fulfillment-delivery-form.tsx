"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayText, lineItemTitle } from "../../lib/readable-label"
import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import { FulfillmentTrackingEntriesField } from "./fulfillment-tracking-entries-field"

type DeliveryDraft = Extract<
    FulfillmentDraft,
    { type: "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" }
>

/** 包裹只在本次配送的真实销售明细下录入，移除只影响该明细的包裹。 */
export function FulfillmentDeliveryForm({
    operation,
    draft,
    onChange,
    disabled,
    compact = false,
}: {
    operation: FulfillmentOperation
    draft: DeliveryDraft
    onChange: (draft: FulfillmentDraft) => void
    disabled?: boolean
    compact?: boolean
}) {
    const kind = draft.type === "WAREHOUSE_SHIP" ? "ship" : "direct"
    const prefix = `fulfillment-operations-${kind}-form`
    const entries = draft.trackingEntries ?? []
    const actualIds = new Set(draft.lines.map((line) => line.salesOrderLineId))
    const orphaned = entries.filter(
        (entry) => !actualIds.has(entry.salesOrderLineId),
    )
    return (
        <div
            className="space-y-5"
            aria-label={
                draft.type === "WAREHOUSE_SHIP"
                    ? "公司仓发表单"
                    : "供应商直发表单"
            }
        >
            {draft.type === "WAREHOUSE_SHIP" &&
            displayText(draft.warehouseLabel, draft.warehouseId) ? (
                <div className="space-y-1.5">
                    <Label htmlFor={`${prefix}-warehouse`}>发货仓</Label>
                    <Input
                        id={`${prefix}-warehouse`}
                        value={displayText(
                            draft.warehouseLabel,
                            draft.warehouseId,
                        )}
                        disabled
                        readOnly
                    />
                </div>
            ) : null}
            <section className="space-y-3">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <h3 className="text-sm font-semibold">
                        {compact ? "商品与物流" : "发货明细与物流号"}
                    </h3>
                    {!compact ? (
                        <p className="num text-xs text-muted-foreground">
                            {actualIds.size} 项商品
                        </p>
                    ) : null}
                </div>
                <div
                    className={
                        compact
                            ? "divide-y divide-border overflow-hidden rounded-lg border border-grid"
                            : "space-y-3"
                    }
                >
                    {[...actualIds].map((salesOrderLineId, index) => {
                        const source = operation.lines.find(
                            (line) =>
                                line.salesOrderLineId === salesOrderLineId,
                        )
                        const lines = draft.lines.filter(
                            (line) =>
                                line.salesOrderLineId === salesOrderLineId,
                        )
                        const linePrefix = `${prefix}-line-${toAutomationIdSegment(salesOrderLineId)}`
                        return (
                            <div
                                key={`${operation.operationId}:${salesOrderLineId}`}
                                className={
                                    compact
                                        ? "space-y-3 p-4"
                                        : "space-y-3 rounded-xl border border-border p-4"
                                }
                            >
                                <div className="flex flex-wrap items-start justify-between gap-2">
                                    <p className="min-w-0 text-sm font-medium">
                                        {lineItemTitle(source?.itemName, index)}{" "}
                                        {displayText(source?.skuCode) ? (
                                            <span className="num text-xs font-normal text-muted-foreground">
                                                {displayText(source?.skuCode)}
                                            </span>
                                        ) : null}
                                    </p>
                                    <p className="shrink-0 text-xs text-muted-foreground">
                                        本次发货{" "}
                                        <span className="num font-medium text-foreground">
                                            {compactFixed(
                                                sumFixed(
                                                    lines.map(
                                                        (line) => line.quantity,
                                                    ),
                                                    {
                                                        maxScale: 6,
                                                        outputScale: 6,
                                                    },
                                                ),
                                            )}
                                        </span>
                                        {displayText(source?.unitCode)}
                                    </p>
                                </div>
                                <FulfillmentTrackingEntriesField
                                    id={linePrefix}
                                    salesOrderLineId={salesOrderLineId}
                                    entries={entries}
                                    disabled={disabled}
                                    compact={compact}
                                    onChange={(trackingEntries, inputAdded) =>
                                        onChange({
                                            ...draft,
                                            trackingEntries,
                                            pendingTrackingLineIds: inputAdded
                                                ? draft.pendingTrackingLineIds?.filter(
                                                      (id) =>
                                                          id !==
                                                          salesOrderLineId,
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
                                                      (id) =>
                                                          id !==
                                                          salesOrderLineId,
                                                  ),
                                        })
                                    }
                                />
                            </div>
                        )
                    })}
                </div>
            </section>
            {orphaned.length ? (
                <div className="space-y-2 rounded-lg border border-destructive p-3">
                    <p role="alert" className="text-sm text-destructive">
                        有 {orphaned.length}{" "}
                        个物流号不属于当前发货明细。请移除后重新登记，其他明细的物流号保持原归属。
                    </p>
                    <Button
                        id={`${prefix}-remove-unselected-tracking`}
                        type="button"
                        variant="outline"
                        disabled={disabled}
                        onClick={() =>
                            onChange({
                                ...draft,
                                trackingEntries: entries.filter((entry) =>
                                    actualIds.has(entry.salesOrderLineId),
                                ),
                            })
                        }
                    >
                        移除不属于当前明细的物流号
                    </Button>
                </div>
            ) : null}
        </div>
    )
}
