"use client"

import { OptionCombobox } from "@/components/business"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import type { FulfillmentDraft, FulfillmentTask } from "./types"

export function FulfillmentDirectForm({
    task,
    draft,
    onChange,
    disabled,
}: {
    task: FulfillmentTask
    draft: Extract<FulfillmentDraft, { type: "SUPPLIER_DIRECT" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <section className="space-y-3" aria-label="供应商直发表单">
            <h3 className="text-sm font-semibold">供应商直发</h3>
            <p className="text-xs text-muted-foreground">
                供应商直接发给客户，不走自己的仓库，库存不变。
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor="direct-carrier">承运方</Label>
                    <OptionCombobox
                        id="direct-carrier"
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
                    <Label htmlFor="direct-tracking">物流单号</Label>
                    <Input
                        id="direct-tracking"
                        value={draft.trackingNo}
                        disabled={disabled}
                        onChange={(e) =>
                            onChange({ ...draft, trackingNo: e.target.value })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="direct-at">发货时间</Label>
                    <DateTimeLocalPicker
                        value={draft.shippedAt || undefined}
                        disabled={disabled}
                        onValueChange={(next) =>
                            onChange({ ...draft, shippedAt: next ?? "" })
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
                        className="space-y-2 rounded-xl border border-border p-3"
                    >
                        <p className="text-sm font-medium">{src?.itemName}</p>
                        <p className="text-xs text-muted-foreground">
                            对应 {task.source.salesOrderNo} · 还剩{" "}
                            <span className="num">
                                {src?.remainingQuantity}
                            </span>
                            {src?.unitCode} 没发
                        </p>
                        <div className="space-y-1.5">
                            <Label htmlFor={`direct-qty-${i}`}>发货数量</Label>
                            <Input
                                id={`direct-qty-${i}`}
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
