"use client"

import { OptionCombobox } from "@/components/business"
import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { QUALITY_RESULT_OPTIONS } from "@/lib/business-options"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { withDerivedQualified } from "@/features/fulfillment-operations/lib/validation"
import {
    displayText,
    lineItemTitle,
} from "@/features/fulfillment-operations/lib/readable-label"
import { toAutomationIdSegment } from "@/lib/automation-id"

/**
 * 采购入库表单。PurchaseReceipt 为 NO_APPROVAL，只收集仓、时间与到货数量，
 * 不嵌入绑定卡、决定弹窗、撤回或改派入口。
 *
 * @param operation 入库工作单。
 * @param draft 入库草稿。
 * @param onChange 草稿变更回调。
 * @param disabled 只读或提交中禁用。
 */
export function FulfillmentReceiptForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "RECEIPT" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <section className="space-y-3" aria-label="入库表单">
            <h3 className="text-sm font-semibold">入库作业</h3>
            <div className="grid gap-3 sm:grid-cols-2">
                <div className="space-y-1.5">
                    <Label htmlFor="fulfillment-operations-receipt-form-warehouse">
                        入库仓
                    </Label>
                    <Input
                        id="fulfillment-operations-receipt-form-warehouse"
                        value={
                            displayText(draft.warehouseLabel) || "待核对仓库"
                        }
                        disabled
                        readOnly
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="fulfillment-operations-receipt-form-occurred-at">
                        入库时间
                    </Label>
                    <DateTimeLocalPicker
                        id="fulfillment-operations-receipt-form-occurred-at"
                        value={draft.occurredAt || undefined}
                        disabled={disabled}
                        showTimeZone={false}
                        onValueChange={(next) =>
                            onChange({ ...draft, occurredAt: next ?? "" })
                        }
                    />
                </div>
            </div>
            {draft.lines.map((line, i) => {
                const src = operation.lines.find(
                    (l) =>
                        l.purchaseRevisionLineId ===
                        line.purchaseRevisionLineId,
                )
                return (
                    <div
                        key={line.purchaseRevisionLineId}
                        className="space-y-3 rounded-xl border border-border p-3"
                    >
                        <p className="text-sm font-medium">
                            {lineItemTitle(src?.itemName, i)}
                            {displayText(src?.remainingQuantity)
                                ? ` · 剩余可收 ${src?.remainingQuantity}${displayText(src?.unitCode)}`
                                : ""}
                        </p>
                        <div className="grid gap-3 sm:grid-cols-3">
                            <div className="space-y-1.5">
                                <Label
                                    htmlFor={`fulfillment-operations-receipt-form-received-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                >
                                    到货数量
                                </Label>
                                <Input
                                    id={`fulfillment-operations-receipt-form-received-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                    className="num"
                                    inputMode="decimal"
                                    value={line.receivedQuantity}
                                    disabled={disabled}
                                    onChange={(e) => {
                                        const lines = draft.lines.map(
                                            (l, idx) =>
                                                idx === i
                                                    ? withDerivedQualified({
                                                          ...l,
                                                          receivedQuantity:
                                                              e.target.value,
                                                      })
                                                    : l,
                                        )
                                        onChange({ ...draft, lines })
                                    }}
                                />
                            </div>
                            <div className="space-y-1.5">
                                <Label
                                    htmlFor={`fulfillment-operations-receipt-form-qualified-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                >
                                    合格数量
                                    <span className="ml-1 font-normal text-muted-foreground">
                                        自动算
                                    </span>
                                </Label>
                                <Input
                                    id={`fulfillment-operations-receipt-form-qualified-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                    className="num"
                                    inputMode="decimal"
                                    value={line.qualifiedQuantity}
                                    disabled={disabled}
                                    onChange={(e) => {
                                        const lines = draft.lines.map(
                                            (l, idx) =>
                                                idx === i
                                                    ? {
                                                          ...l,
                                                          qualifiedQuantity:
                                                              e.target.value,
                                                      }
                                                    : l,
                                        )
                                        onChange({ ...draft, lines })
                                    }}
                                />
                            </div>
                            <div className="space-y-1.5">
                                <Label
                                    htmlFor={`fulfillment-operations-receipt-form-rejected-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                >
                                    不合格数量
                                </Label>
                                <Input
                                    id={`fulfillment-operations-receipt-form-rejected-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                    className="num"
                                    inputMode="decimal"
                                    value={line.rejectedQuantity}
                                    disabled={disabled}
                                    onChange={(e) => {
                                        const lines = draft.lines.map(
                                            (l, idx) =>
                                                idx === i
                                                    ? withDerivedQualified({
                                                          ...l,
                                                          rejectedQuantity:
                                                              e.target.value,
                                                      })
                                                    : l,
                                        )
                                        onChange({ ...draft, lines })
                                    }}
                                />
                            </div>
                        </div>
                        <div className="space-y-1.5">
                            <Label
                                htmlFor={`fulfillment-operations-receipt-form-quality-result-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                            >
                                质量结果
                            </Label>
                            <OptionCombobox
                                id={`fulfillment-operations-receipt-form-quality-result-${toAutomationIdSegment(line.purchaseRevisionLineId)}`}
                                value={line.qualityResult || null}
                                disabled={disabled}
                                options={QUALITY_RESULT_OPTIONS}
                                allowClear={false}
                                placeholder="选择质量结果"
                                onValueChange={(v) => {
                                    const lines = draft.lines.map((l, idx) =>
                                        idx === i
                                            ? { ...l, qualityResult: v ?? "" }
                                            : l,
                                    )
                                    onChange({ ...draft, lines })
                                }}
                            />
                        </div>
                        <p className="text-xs text-muted-foreground">
                            只有合格的货入库并留给对应的销售单；不合格的不进库存。
                        </p>
                    </div>
                )
            })}
        </section>
    )
}
