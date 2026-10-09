"use client"

import { z } from "zod"
import { PlusIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { InputGroup } from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    MAX_TRACKING_NUMBER_LENGTH,
    normalizeTrackingNumbers,
} from "../../lib/tracking-numbers"
import { trackingEntryKey } from "../../lib/tracking-entries"
import type { DeliveryTrackingEntry } from "../../types"

const packageSchema = z.object({
    trackingNo: z
        .string()
        .trim()
        .min(1, "请填写物流号")
        .refine(
            (value) => normalizeTrackingNumbers(value).length > 0,
            "请填写物流号",
        )
        .refine(
            (value) =>
                normalizeTrackingNumbers(value).every(
                    (number) => number.length <= MAX_TRACKING_NUMBER_LENGTH,
                ),
            `每个物流号最多${MAX_TRACKING_NUMBER_LENGTH}个字符`,
        ),
    carrier: z.string().max(64, "承运方最多64个字符"),
})
const emptyPackageValues = { trackingNo: "", carrier: "" }

/** 在明确的销售明细下录入包裹，可分批选择不同承运方。 */
export function FulfillmentTrackingEntriesField({
    id,
    salesOrderLineId,
    entries,
    onChange,
    onPendingInputChange,
    disabled,
    compact = false,
}: {
    id: string
    salesOrderLineId: string
    entries: readonly DeliveryTrackingEntry[]
    onChange: (entries: DeliveryTrackingEntry[], inputAdded?: boolean) => void
    onPendingInputChange: (pending: boolean) => void
    disabled?: boolean
    compact?: boolean
}) {
    const packages = entries.filter(
        (entry) => entry.salesOrderLineId === salesOrderLineId,
    )
    const form = useAppForm({
        defaultValues: emptyPackageValues,
        validators: { onChange: packageSchema },
        onSubmit: ({ value }) => {
            if (disabled) return
            const next = normalizeTrackingNumbers(value.trackingNo).map(
                (trackingNo) => ({
                    salesOrderLineId,
                    trackingNo,
                    carrier: value.carrier.trim() || undefined,
                }),
            )
            const seen = new Set(entries.map(trackingEntryKey))
            onChange(
                [
                    ...entries,
                    ...next.filter((entry) => {
                        const key = trackingEntryKey(entry)
                        if (seen.has(key)) return false
                        seen.add(key)
                        return true
                    }),
                ],
                true,
            )
            // 只重置物流号，保留承运方字段及下拉选择状态。
            form.resetField("trackingNo")
        },
    })
    return (
        <div className="space-y-3 border-t border-border pt-3">
            {!compact ? (
                <>
                    <p className="text-sm font-medium">
                        本明细物流号 · {packages.length} 个包裹
                    </p>
                    <p className="text-xs text-muted-foreground">
                        同一明细可登记多个包裹。一个包裹包含多项明细时，请在对应明细下分别登记同一物流号。
                    </p>
                </>
            ) : null}
            {packages.length ? (
                <ul className="space-y-2">
                    {packages.map((entry) => (
                        <li
                            key={trackingEntryKey(entry)}
                            className="flex items-center justify-between gap-2 rounded-md bg-muted/40 px-3 py-2 text-sm"
                        >
                            <div className="min-w-0">
                                <span className="num break-all">
                                    {entry.trackingNo}
                                </span>
                                <span className="ml-2 text-xs text-muted-foreground">
                                    {entry.carrier || "承运方未填写"}
                                </span>
                            </div>
                            <Button
                                id={`${id}-remove-${toAutomationIdSegment(trackingEntryKey(entry))}`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={disabled}
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
            ) : !compact ? (
                <p className="text-xs text-muted-foreground">
                    请添加本明细的物流号。
                </p>
            ) : null}
            <form
                id={`${id}-form`}
                className="grid gap-3"
                onSubmit={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    void form.handleSubmit()
                }}
            >
                <Label htmlFor={`${id}-tracking-no`}>物流信息</Label>
                <InputGroup
                    className="h-auto items-stretch"
                    aria-label="物流公司与快递单号"
                    data-disabled={disabled || undefined}
                >
                    <form.AppField
                        name="carrier"
                        children={(field) => (
                            <field.SelectField
                                id={`${id}-carrier`}
                                label="物流公司（可选）"
                                options={CARRIER_OPTIONS}
                                disabled={disabled}
                                allowClear
                                hideLabel
                                inInputGroup
                                className="w-40 shrink-0 border-r border-border sm:w-48"
                                placeholder="选择物流公司"
                            />
                        )}
                    />
                    <form.AppField
                        name="trackingNo"
                        listeners={{
                            onChange: ({ value }) =>
                                onPendingInputChange(Boolean(value.trim())),
                        }}
                        children={(field) => (
                            <field.TextareaField
                                id={`${id}-tracking-no`}
                                label="快递单号"
                                placeholder="输入快递单号，多个号码可分行粘贴"
                                rows={1}
                                hideLabel
                                inInputGroup
                                className="min-w-0 flex-1"
                                required
                                disabled={disabled}
                            />
                        )}
                    />
                </InputGroup>
                <form.AppForm>
                    <form.SubmitButton
                        id={`${id}-add`}
                        label="添加物流号"
                        variant={compact ? "ghost" : "default"}
                        size={compact ? "sm" : "default"}
                        className={compact ? "justify-self-start" : undefined}
                        disabled={disabled}
                    >
                        {compact ? (
                            <>
                                <PlusIcon data-icon="inline-start" />
                                添加物流号
                            </>
                        ) : undefined}
                    </form.SubmitButton>
                </form.AppForm>
            </form>
            {compact ? (
                <p className="text-xs text-muted-foreground">
                    同一商品可添加多个物流号；同一包裹包含多项商品时，请在对应商品下分别登记。
                </p>
            ) : null}
        </div>
    )
}
