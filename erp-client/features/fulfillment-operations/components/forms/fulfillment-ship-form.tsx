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
 * 公司仓发表单。Delivery 为 NO_APPROVAL，只收集仓、承运方、物流单号与发货数量，
 * 不嵌入绑定卡、决定弹窗、撤回或改派入口。
 *
 * @param operation 仓发工作单。
 * @param draft 仓发草稿。
 * @param onChange 草稿变更回调。
 * @param disabled 只读或提交中禁用。
 */
export function FulfillmentShipForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "WAREHOUSE_SHIP" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <div className="space-y-5" aria-label="公司仓发表单">
            <section className="space-y-3">
                <h3 className="text-sm font-semibold">物流信息</h3>
                <div className="grid gap-4 sm:grid-cols-2">
                    {displayText(draft.warehouseLabel) ? (
                        <div className="space-y-1.5">
                            <Label htmlFor="fulfillment-operations-ship-form-warehouse">
                                发货仓
                            </Label>
                            <Input
                                id="fulfillment-operations-ship-form-warehouse"
                                value={displayText(draft.warehouseLabel)}
                                disabled
                                readOnly
                            />
                        </div>
                    ) : null}
                    <div className="space-y-1.5">
                        <Label htmlFor="fulfillment-operations-ship-form-carrier">
                            承运方
                            <span className="text-destructive">*</span>
                        </Label>
                        <OptionCombobox
                            id="fulfillment-operations-ship-form-carrier"
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
                        <Label htmlFor="fulfillment-operations-ship-form-tracking-no">
                            物流单号
                            <span className="text-destructive">*</span>
                        </Label>
                        <Input
                            id="fulfillment-operations-ship-form-tracking-no"
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
                    <div className="space-y-1.5">
                        <Label htmlFor="fulfillment-operations-ship-form-shipped-at">
                            发货时间
                        </Label>
                        <DateTimeLocalPicker
                            id="fulfillment-operations-ship-form-shipped-at"
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
                    const reserved = displayText(src?.reservedQuantity)
                    const onHand = displayText(src?.availableOnHand)
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
                                {reserved || onHand ? (
                                    <p className="text-xs text-muted-foreground">
                                        {reserved
                                            ? `已留货 ${reserved}${unit}`
                                            : null}
                                        {reserved && onHand ? " · " : null}
                                        {onHand
                                            ? `仓库现有 ${onHand}${unit}`
                                            : null}
                                    </p>
                                ) : null}
                            </div>
                            <div className="space-y-1.5">
                                <Label
                                    htmlFor={`fulfillment-operations-ship-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`}
                                >
                                    本次发货数量
                                </Label>
                                <Input
                                    id={`fulfillment-operations-ship-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`}
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
