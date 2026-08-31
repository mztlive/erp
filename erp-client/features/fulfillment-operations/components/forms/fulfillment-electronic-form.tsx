"use client"

import { OptionCombobox } from "@/components/business"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type {
    FulfillmentDraft,
    FulfillmentResultCode,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { RESULT_OPTIONS } from "@/features/fulfillment-operations/types"
import {
    displayText,
    lineItemTitle,
} from "@/features/fulfillment-operations/lib/readable-label"
import { toAutomationIdSegment } from "@/lib/automation-id"

/**
 * 电子交付表单。ElectronicDelivery 为 NO_APPROVAL，只收集交付对象、时间、
 * 履约结果与数量，不嵌入绑定卡、决定弹窗、撤回或改派入口。
 *
 * @param operation 电子交付工作单。
 * @param draft 电子交付草稿。
 * @param onChange 草稿变更回调。
 * @param disabled 只读或提交中禁用。
 */
export function FulfillmentElectronicForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "ELECTRONIC" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <section className="space-y-3" aria-label="电子交付表单">
            <h3 className="text-sm font-semibold">电子交付</h3>
            <p className="text-xs text-muted-foreground">
                卡号卡密只显示打码内容，不会存进系统。填了「失败」就不能再改。
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor="fulfillment-operations-electronic-form-recipient">
                        交付对象
                    </Label>
                    <Input
                        id="fulfillment-operations-electronic-form-recipient"
                        value={draft.recipientMasked}
                        disabled
                        readOnly
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="fulfillment-operations-electronic-form-occurred-at">
                        实际时间
                    </Label>
                    <DateTimeLocalPicker
                        id="fulfillment-operations-electronic-form-occurred-at"
                        value={draft.occurredAt || undefined}
                        disabled={disabled}
                        showTimeZone={false}
                        onValueChange={(next) =>
                            onChange({ ...draft, occurredAt: next ?? "" })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="fulfillment-operations-electronic-form-result">
                        履约结果
                    </Label>
                    <OptionCombobox
                        id="fulfillment-operations-electronic-form-result"
                        value={draft.result}
                        onValueChange={(v) =>
                            onChange({
                                ...draft,
                                result: (v ??
                                    draft.result) as FulfillmentResultCode,
                            })
                        }
                        options={RESULT_OPTIONS}
                        allowClear={false}
                        disabled={disabled}
                        aria-label="履约结果"
                        placeholder="请选择履约结果"
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
                        <p className="text-sm font-medium">
                            {lineItemTitle(src?.itemName, i)}
                        </p>
                        {displayText(operation.source.salesOrderNo) ? (
                            <p className="text-xs text-muted-foreground">
                                对应销售单{" "}
                                {displayText(operation.source.salesOrderNo)}
                            </p>
                        ) : null}
                        <div className="space-y-1.5">
                            <Label
                                htmlFor={`fulfillment-operations-electronic-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`}
                            >
                                交付数量
                            </Label>
                            <Input
                                id={`fulfillment-operations-electronic-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`}
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
