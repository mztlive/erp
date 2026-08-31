"use client"

import { OptionCombobox } from "@/components/business"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import {
    displayText,
    lineItemTitle,
} from "@/features/fulfillment-operations/lib/readable-label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/**
 * 供应商直发表单。先填物流，再确认各明细发货数量。
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
        <div className="space-y-5" aria-label="供应商直发表单">
            <section className="space-y-3">
                <header className="space-y-1">
                    <h3 className="text-sm font-semibold">物流信息</h3>
                    <p className="text-xs text-muted-foreground">
                        供应商直接发给客户，不走自有仓库，库存不变。
                    </p>
                </header>
                <div className="grid gap-4 sm:grid-cols-2">
                    <div className="space-y-1.5">
                        <Label htmlFor="fulfillment-operations-direct-form-carrier">
                            承运方
                            <span className="text-destructive">*</span>
                        </Label>
                        <OptionCombobox
                            id="fulfillment-operations-direct-form-carrier"
                            value={draft.carrier || null}
                            onValueChange={(value) =>
                                onChange({
                                    ...draft,
                                    carrier: value ?? "",
                                })
                            }
                            options={CARRIER_OPTIONS}
                            placeholder="选择承运方"
                            allowClear={false}
                            disabled={disabled}
                            aria-label="承运方"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="fulfillment-operations-direct-form-tracking-no">
                            物流单号
                            <span className="text-destructive">*</span>
                        </Label>
                        <Input
                            id="fulfillment-operations-direct-form-tracking-no"
                            value={draft.trackingNo}
                            disabled={disabled}
                            placeholder="请输入物流单号"
                            onChange={(e) =>
                                onChange({
                                    ...draft,
                                    trackingNo: e.target.value,
                                })
                            }
                        />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="fulfillment-operations-direct-form-shipped-at">
                            发货时间
                        </Label>
                        <DateTimeLocalPicker
                            id="fulfillment-operations-direct-form-shipped-at"
                            value={draft.shippedAt || undefined}
                            disabled={disabled}
                            showTimeZone={false}
                            onValueChange={(next) =>
                                onChange({ ...draft, shippedAt: next ?? "" })
                            }
                        />
                    </div>
                </div>
            </section>

            <section className="space-y-3">
                <h3 className="text-sm font-semibold">发货明细</h3>
                {draft.lines.map((line, i) => {
                    const src = operation.lines.find(
                        (l) => l.salesOrderLineId === line.salesOrderLineId,
                    )
                    const remaining = displayText(src?.remainingQuantity)
                    const unit = displayText(src?.unitCode)
                    return (
                        <div
                            key={line.salesOrderLineId}
                            className="space-y-3 rounded-lg border border-border bg-muted/20 p-3"
                        >
                            <div className="space-y-0.5">
                                <p className="text-sm font-medium">
                                    {lineItemTitle(src?.itemName, i)}
                                </p>
                                {remaining ? (
                                    <p className="text-xs text-muted-foreground">
                                        还剩{" "}
                                        <span className="num">{remaining}</span>
                                        {unit} 没发
                                    </p>
                                ) : null}
                            </div>
                            <div className="space-y-1.5">
                                <Label
                                    htmlFor={`fulfillment-operations-direct-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`}
                                >
                                    本次发货数量
                                </Label>
                                <Input
                                    id={`fulfillment-operations-direct-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`}
                                    className="num"
                                    inputMode="decimal"
                                    value={line.quantity}
                                    disabled={disabled}
                                    onChange={(e) => {
                                        const lines = draft.lines.map(
                                            (item, idx) =>
                                                idx === i
                                                    ? {
                                                          ...item,
                                                          quantity:
                                                              e.target.value,
                                                      }
                                                    : item,
                                        )
                                        onChange({ ...draft, lines })
                                    }}
                                />
                            </div>
                        </div>
                    )
                })}
            </section>
        </div>
    )
}
