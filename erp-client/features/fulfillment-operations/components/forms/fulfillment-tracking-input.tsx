"use client"

import { useEffect, useState } from "react"
import { PlusIcon } from "lucide-react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { InputGroup } from "@/components/ui/input-group"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import { appendTrackingEntries } from "../../lib/tracking-entries"
import {
    MAX_TRACKING_NUMBERS,
    MAX_TRACKING_NUMBER_LENGTH,
    normalizeTrackingNumbers,
} from "../../lib/tracking-numbers"
import type { DeliveryTrackingEntry } from "../../types"

/** 单项和批量登记共用录入、去重和数量限制；未添加输入不进入发货记录。 */
export function FulfillmentTrackingInput({
    id,
    salesOrderLineIds,
    entries,
    onChange,
    onPendingInputChange,
    pendingInput,
    disabled,
    batch = false,
}: {
    id: string
    salesOrderLineIds: readonly string[]
    entries: readonly DeliveryTrackingEntry[]
    onChange: (entries: DeliveryTrackingEntry[]) => void
    onPendingInputChange: (pending: boolean) => void
    pendingInput: boolean
    disabled?: boolean
    batch?: boolean
}) {
    const [message, setMessage] = useState("")
    const schema = z
        .object({
            carrier: z.string().max(64, "承运方最多64个字符"),
            trackingNo: z.string().superRefine((value, ctx) => {
                const numbers = normalizeTrackingNumbers(value)
                if (!numbers.length) {
                    ctx.addIssue({ code: "custom", message: "请填写物流号" })
                } else if (
                    numbers.some(
                        (number) => number.length > MAX_TRACKING_NUMBER_LENGTH,
                    )
                ) {
                    ctx.addIssue({
                        code: "custom",
                        message: `每个物流号最多${MAX_TRACKING_NUMBER_LENGTH}个字符`,
                    })
                }
            }),
        })
        .superRefine((value, ctx) => {
            if (!salesOrderLineIds.length) {
                ctx.addIssue({
                    code: "custom",
                    path: ["trackingNo"],
                    message: "请先勾选要登记物流的商品",
                })
            }
            if (
                appendTrackingEntries(
                    entries,
                    salesOrderLineIds,
                    normalizeTrackingNumbers(value.trackingNo),
                    value.carrier,
                ).length > MAX_TRACKING_NUMBERS
            ) {
                ctx.addIssue({
                    code: "custom",
                    path: ["trackingNo"],
                    message: `本次发货最多添加 ${MAX_TRACKING_NUMBERS} 条包裹明细，请减少商品或单号数量`,
                })
            }
        })
    const form = useAppForm({
        defaultValues: { trackingNo: "", carrier: "" },
        validators: { onChange: schema },
        onSubmit: ({ value }) => {
            if (disabled || !salesOrderLineIds.length) return
            const next = appendTrackingEntries(
                entries,
                salesOrderLineIds,
                normalizeTrackingNumbers(value.trackingNo),
                value.carrier,
            )
            onChange(next)
            setMessage(
                next.length === entries.length
                    ? "所填物流已登记，未重复添加"
                    : batch
                      ? `已为所选 ${salesOrderLineIds.length} 项商品追加 ${next.length - entries.length} 条物流记录`
                      : `已添加 ${next.length - entries.length} 条物流记录`,
            )
            form.resetField("trackingNo")
        },
    })
    // 放弃草稿或外部重置时同步清空输入；普通折叠和筛选不会触发重置。
    useEffect(() => {
        if (!pendingInput) form.resetField("trackingNo")
    }, [form, pendingInput])
    const validationContext = JSON.stringify([entries, salesOrderLineIds])
    useEffect(() => {
        if (form.state.values.trackingNo.trim()) void form.validate("change")
    }, [form, validationContext])

    return (
        <form
            id={`${id}-form`}
            className="space-y-2"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                void form.handleSubmit()
            }}
        >
            <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
                <InputGroup
                    className="h-auto w-full min-w-0 flex-1 flex-col items-stretch sm:flex-row"
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
                                className="w-full shrink-0 border-b border-border sm:w-40 sm:border-b-0 sm:border-r"
                                placeholder="选择物流公司"
                            />
                        )}
                    />
                    <form.AppField
                        name="trackingNo"
                        listeners={{
                            onChange: ({ value }) => {
                                setMessage("")
                                onPendingInputChange(Boolean(value.trim()))
                            },
                        }}
                        children={(field) => (
                            <field.TextareaField
                                id={`${id}-tracking-no`}
                                label="快递单号"
                                placeholder="输入单号，按 Enter 添加"
                                onKeyDown={(event) => {
                                    if (
                                        event.key !== "Enter" ||
                                        event.shiftKey ||
                                        event.ctrlKey ||
                                        event.altKey ||
                                        event.metaKey ||
                                        event.nativeEvent.isComposing ||
                                        event.keyCode === 229
                                    )
                                        return
                                    event.preventDefault()
                                    event.stopPropagation()
                                    if (!disabled && !event.repeat)
                                        void form.handleSubmit()
                                }}
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
                        label={batch ? "追加到所选商品" : "添加物流号"}
                        aria-label={batch ? "追加到所选商品" : "添加物流号"}
                        variant="outline"
                        className="w-full shrink-0 sm:w-auto"
                        disabled={disabled || !salesOrderLineIds.length}
                    >
                        <PlusIcon data-icon="inline-start" />
                        {batch ? "追加到所选商品" : "添加"}
                    </form.SubmitButton>
                </form.AppForm>
            </div>
            {message ? (
                <p role="status" className="text-xs text-muted-foreground">
                    {message}
                </p>
            ) : null}
        </form>
    )
}
