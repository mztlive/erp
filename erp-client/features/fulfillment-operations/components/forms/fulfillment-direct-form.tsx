"use client"

import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/**
 * 供应商直发表单。Delivery 为 NO_APPROVAL，只收集承运方、物流单号与发货数量，
 * 不嵌入绑定卡、决定弹窗、撤回或改派入口。
 *
 * @param operation 直发工作单。
 * @param draft 直发草稿。
 * @param onChange 草稿变更回调。
 * @param disabled 只读或提交中禁用。
 */
export function FulfillmentDirectForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
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
                    <Label htmlFor="direct-carrier">承运方（必填）</Label>
                    <div
                        id="direct-carrier"
                        tabIndex={-1}
                        role="radiogroup"
                        aria-label="承运方"
                        className="flex flex-wrap gap-2 outline-none"
                    >
                        {CARRIER_OPTIONS.map((option) => (
                            <Button
                                key={option.value}
                                type="button"
                                size="sm"
                                variant={
                                    draft.carrier === option.value
                                        ? "default"
                                        : "outline"
                                }
                                disabled={disabled}
                                aria-pressed={draft.carrier === option.value}
                                onClick={() =>
                                    onChange({
                                        ...draft,
                                        carrier: option.value,
                                    })
                                }
                            >
                                {option.label}
                            </Button>
                        ))}
                    </div>
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
                const src = operation.lines.find(
                    (l) => l.salesOrderLineId === line.salesOrderLineId,
                )
                return (
                    <div
                        key={line.salesOrderLineId}
                        className="space-y-2 rounded-xl border border-border p-3"
                    >
                        <p className="text-sm font-medium">{src?.itemName}</p>
                        <p className="text-xs text-muted-foreground">
                            对应 {operation.source.salesOrderNo} · 还剩{" "}
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
