"use client"

import { OptionCombobox } from "@/components/business"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import type { FulfillmentDraft, FulfillmentTask } from "./types"

export function FulfillmentShipForm({
    task,
    draft,
    onChange,
    disabled,
}: {
    task: FulfillmentTask
    draft: Extract<FulfillmentDraft, { type: "WAREHOUSE_SHIP" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <section className="space-y-3" aria-label="公司仓发表单">
            <h3 className="text-sm font-semibold">公司仓发</h3>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label>发货仓</Label>
                    <Input value={draft.warehouseLabel} disabled readOnly />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="ship-at">发货时间</Label>
                    <DateTimeLocalPicker
                        value={draft.shippedAt || undefined}
                        disabled={disabled}
                        onValueChange={(next) =>
                            onChange({ ...draft, shippedAt: next ?? "" })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="ship-carrier">承运方</Label>
                    <OptionCombobox
                        id="ship-carrier"
                        value={draft.carrier || null}
                        disabled={disabled}
                        options={CARRIER_OPTIONS}
                        allowClear={false}
                        placeholder="选择承运方"
                        onValueChange={(v) =>
                            onChange({ ...draft, carrier: v ?? "" })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="ship-tracking">物流单号</Label>
                    <Input
                        id="ship-tracking"
                        value={draft.trackingNo}
                        disabled={disabled}
                        onChange={(e) =>
                            onChange({ ...draft, trackingNo: e.target.value })
                        }
                    />
                </div>
            </div>
            {draft.lines.map((line, i) => {
                const src = task.lines.find(
                    (l) => l.salesOrderLineId === line.salesOrderLineId,
                )
                return (
                    <div
                        key={line.salesOrderLineId}
                        className="space-y-3 rounded-xl border border-border p-3"
                    >
                        <p className="text-sm font-medium">{src?.itemName}</p>
                        <p className="text-xs text-muted-foreground">
                            已为 {task.source.salesOrderNo} 留货{" "}
                            <span className="num">{src?.reservedQuantity}</span>
                            {src?.unitCode} · 仓库现有{" "}
                            <span className="num">{src?.availableOnHand}</span>
                            {src?.unitCode}
                        </p>
                        <div className="space-y-1.5">
                            <Label htmlFor={`ship-qty-${i}`}>这次发多少</Label>
                            <Input
                                id={`ship-qty-${i}`}
                                className="num"
                                inputMode="decimal"
                                value={line.quantity}
                                disabled={disabled}
                                onChange={(e) => {
                                    const lines = draft.lines.map((l, idx) =>
                                        idx === i
                                            ? { ...l, quantity: e.target.value }
                                            : l,
                                    )
                                    onChange({ ...draft, lines })
                                }}
                            />
                        </div>
                    </div>
                )
            })}
        </section>
    )
}
