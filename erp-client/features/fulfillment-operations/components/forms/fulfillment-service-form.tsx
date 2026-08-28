"use client"

import { DateTimeLocalPicker } from "@/components/ui/date-picker"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import {
    displayText,
    lineItemTitle,
} from "@/features/fulfillment-operations/lib/readable-label"
import type {
    FulfillmentDraft,
    FulfillmentResultCode,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { RESULT_OPTIONS } from "@/features/fulfillment-operations/types"

/**
 * 线下服务表单。先登记现场情况，再确认各服务项目数量。
 *
 * @param operation 服务履约工作单。
 * @param draft 线下服务草稿。
 * @param onChange 草稿变更回调。
 * @param disabled 只读或提交中禁用。
 */
export function FulfillmentServiceForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "SERVICE" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return (
        <div className="space-y-5" aria-label="线下服务表单">
            <section className="space-y-3">
                <header className="space-y-1">
                    <h3 className="text-sm font-semibold">服务情况</h3>
                    <p className="text-xs text-muted-foreground">
                        到客户现场完成安装、调试或培训，不走仓库。
                    </p>
                </header>
                <div className="grid gap-4 sm:grid-cols-2">
                    <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="service-loc">
                            服务地点
                            <span className="text-destructive">*</span>
                        </Label>
                        <Input
                            id="service-loc"
                            value={draft.serviceLocation}
                            disabled={disabled}
                            placeholder="客户现场或安装地址"
                            onChange={(e) =>
                                onChange({
                                    ...draft,
                                    serviceLocation: e.target.value,
                                })
                            }
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="service-start">
                            开始时间
                            <span className="text-destructive">*</span>
                        </Label>
                        <DateTimeLocalPicker
                            id="service-start"
                            value={draft.startedAt || undefined}
                            disabled={disabled}
                            showTimeZone={false}
                            placeholder="选择开始时间"
                            onValueChange={(next) =>
                                onChange({ ...draft, startedAt: next ?? "" })
                            }
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="service-ended">
                            结束时间
                            <span className="text-destructive">*</span>
                        </Label>
                        <DateTimeLocalPicker
                            id="service-ended"
                            value={draft.endedAt || undefined}
                            disabled={disabled}
                            showTimeZone={false}
                            placeholder="选择结束时间"
                            onValueChange={(next) =>
                                onChange({ ...draft, endedAt: next ?? "" })
                            }
                        />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                        <Label id="svc-result-label">履约结果</Label>
                        <div
                            id="svc-result"
                            tabIndex={-1}
                            role="radiogroup"
                            aria-labelledby="svc-result-label"
                            className="grid grid-cols-3 gap-2 outline-none"
                        >
                            {RESULT_OPTIONS.map((option) => (
                                <Button
                                    key={option.value}
                                    type="button"
                                    size="sm"
                                    variant={
                                        draft.result === option.value
                                            ? "default"
                                            : "outline"
                                    }
                                    disabled={disabled}
                                    aria-pressed={draft.result === option.value}
                                    className="rounded-lg shadow-none"
                                    onClick={() =>
                                        onChange({
                                            ...draft,
                                            result: option.value as FulfillmentResultCode,
                                        })
                                    }
                                >
                                    {option.label}
                                </Button>
                            ))}
                        </div>
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="service-note">
                            完成说明
                            <span className="text-destructive">*</span>
                        </Label>
                        <Textarea
                            id="service-note"
                            value={draft.completionNote}
                            disabled={disabled}
                            rows={3}
                            placeholder="例如：已上门安装并完成现场验收"
                            onChange={(e) =>
                                onChange({
                                    ...draft,
                                    completionNote: e.target.value,
                                })
                            }
                        />
                    </div>
                </div>
            </section>

            <section className="space-y-3">
                <h3 className="text-sm font-semibold">服务项目</h3>
                {draft.lines.map((line, i) => {
                    const src = operation.lines.find(
                        (item) =>
                            item.salesOrderLineId === line.salesOrderLineId,
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
                                        {unit} 待完成
                                    </p>
                                ) : null}
                            </div>
                            <div className="space-y-1.5">
                                <Label htmlFor={`svc-qty-${i}`}>
                                    本次完成数量
                                </Label>
                                <Input
                                    id={`svc-qty-${i}`}
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
