"use client"

import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { trackingEntryKey } from "../../lib/tracking-entries"
import type { DeliveryTrackingEntry } from "../../types"
import { FulfillmentTrackingInput } from "./fulfillment-tracking-input"

/** 在商品行内编辑物流；移除只影响当前商品，输入由常驻表单保留。 */
export function FulfillmentTrackingEntriesField({
    id,
    salesOrderLineId,
    entries,
    onChange,
    onPendingInputChange,
    pendingInput,
    disabled,
}: {
    id: string
    salesOrderLineId: string
    entries: readonly DeliveryTrackingEntry[]
    onChange: (entries: DeliveryTrackingEntry[], inputAdded?: boolean) => void
    onPendingInputChange: (pending: boolean) => void
    pendingInput: boolean
    disabled?: boolean
}) {
    const packages = entries.filter(
        (entry) => entry.salesOrderLineId === salesOrderLineId,
    )
    return (
        <div className="space-y-3">
            {packages.length ? (
                <ul
                    aria-label="本商品已登记的物流"
                    className="max-h-48 divide-y divide-border overflow-y-auto"
                >
                    {packages.map((entry) => (
                        <li
                            key={trackingEntryKey(entry)}
                            className="flex items-center gap-3 py-1"
                        >
                            <span className="w-24 shrink-0 break-words text-xs text-muted-foreground">
                                {entry.carrier || "未填写物流公司"}
                            </span>
                            <span className="num min-w-0 flex-1 break-all text-sm">
                                {entry.trackingNo}
                            </span>
                            <Button
                                id={`${id}-remove-${toAutomationIdSegment(trackingEntryKey(entry))}`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={disabled}
                                aria-label={`移除${entry.carrier || ""}物流号 ${entry.trackingNo}`}
                                onClick={() =>
                                    onChange(
                                        entries.filter(
                                            (item) =>
                                                trackingEntryKey(item) !==
                                                trackingEntryKey(entry),
                                        ),
                                    )
                                }
                            >
                                移除
                            </Button>
                        </li>
                    ))}
                </ul>
            ) : null}
            <FulfillmentTrackingInput
                id={id}
                salesOrderLineIds={[salesOrderLineId]}
                entries={entries}
                disabled={disabled}
                onChange={(next) => onChange(next, true)}
                onPendingInputChange={onPendingInputChange}
                pendingInput={pendingInput}
            />
        </div>
    )
}
