"use client"

import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
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
            <section className="space-y-3">
                {!compact ? (
                    <h3 className="text-sm font-semibold">发货信息</h3>
                ) : null}
                {!compact && draft.type === "SUPPLIER_DIRECT" ? (
                    <p className="text-xs text-muted-foreground">
                        供应商直接发给客户，不走自有仓库，库存不变。
                    </p>
                ) : null}
                <div className="grid items-center gap-4 sm:grid-cols-2">
                    {draft.type === "WAREHOUSE_SHIP" &&
                    displayText(draft.warehouseLabel, draft.warehouseId) ? (
                        <div className="space-y-1.5">
                            <Label htmlFor={`${prefix}-warehouse`}>
                                发货仓
                            </Label>
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
                    <div
                        className={
                            compact
                                ? "flex flex-wrap items-center gap-3"
                                : "space-y-1.5"
                        }
                    >
                        <Label htmlFor={`${prefix}-shipped-at`}>发货时间</Label>
                        <div className={compact ? "min-w-0 flex-1" : undefined}>
                            <DateTimeLocalPicker
                                id={`${prefix}-shipped-at`}
                                value={draft.shippedAt || undefined}
                                disabled={disabled}
                                showTimeZone={false}
                                onValueChange={(value) =>
                                    onChange({
                                        ...draft,
                                        shippedAt: value ?? "",
                                    })
                                }
                            />
                        </div>
                    </div>
                    {compact && draft.type === "SUPPLIER_DIRECT" ? (
                        <p className="text-xs text-muted-foreground">
                            供应商直接发给客户，不走自有仓库。
                        </p>
                    ) : null}
                </div>
            </section>
            <section className="space-y-3">
                <h3 className="text-sm font-semibold">
                    {compact ? "商品与物流" : "发货明细与物流号"}
                </h3>
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
                                        : "space-y-3 rounded-xl border border-border p-3"
                                }
                            >
                                <div
                                    className={cn(
                                        compact
                                            ? "flex flex-wrap items-start justify-between gap-2"
                                            : "space-y-1",
                                    )}
                                >
                                    <p className="text-sm font-medium">
                                        {lineItemTitle(source?.itemName, index)}{" "}
                                        {displayText(source?.skuCode) ? (
                                            <span className="num text-xs text-muted-foreground">
                                                {displayText(source?.skuCode)}
                                            </span>
                                        ) : null}
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        本次发货{" "}
                                        {compactFixed(
                                            sumFixed(
                                                lines.map(
                                                    (line) => line.quantity,
                                                ),
                                                { maxScale: 6, outputScale: 6 },
                                            ),
                                        )}
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
