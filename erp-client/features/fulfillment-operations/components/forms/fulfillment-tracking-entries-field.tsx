"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { z } from "zod"
import { PlusIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { InputGroup } from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
    TableCaption,
} from "@/components/ui/table"
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
    compact: _compact = false,
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
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <Label htmlFor={`${id}-tracking-no`}>
                    物流公司与快递单号
                    <RequiredMark />
                </Label>
                <p className="num text-xs text-muted-foreground">
                    {packages.length} 个包裹
                </p>
            </div>
            {packages.length ? (
                <div className="overflow-hidden rounded-lg border border-grid">
                    <Table className="w-full table-fixed text-left text-sm">
                        <TableCaption className="sr-only">
                            本商品已登记的物流包裹
                        </TableCaption>
                        <TableHeader className="bg-muted/30 text-xs text-muted-foreground">
                            <TableRow>
                                <TableHead
                                    scope="col"
                                    className="w-28 px-3 py-2 font-medium sm:w-40"
                                >
                                    物流公司
                                </TableHead>
                                <TableHead
                                    scope="col"
                                    className="px-3 py-2 font-medium"
                                >
                                    快递单号
                                </TableHead>
                                <TableHead
                                    scope="col"
                                    className="w-20 px-3 py-2 text-right font-medium"
                                >
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody className="divide-y divide-border">
                            {packages.map((entry) => (
                                <TableRow key={trackingEntryKey(entry)}>
                                    <TableCell className="break-words whitespace-normal px-3 py-1.5 text-muted-foreground">
                                        {entry.carrier || "未填写"}
                                    </TableCell>
                                    <TableCell className="num break-all whitespace-normal px-3 py-1.5">
                                        {entry.trackingNo}
                                    </TableCell>
                                    <TableCell className="px-2 py-1 text-right">
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
                                                            trackingEntryKey(
                                                                item,
                                                            ) !==
                                                            trackingEntryKey(
                                                                entry,
                                                            ),
                                                    ),
                                                )
                                            }
                                        >
                                            移除
                                        </Button>
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
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
                <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
                    <InputGroup
                        className="h-auto w-full min-w-0 flex-1 flex-col items-stretch sm:flex-row sm:items-stretch"
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
                                    className="w-full shrink-0 border-b border-border sm:w-48 sm:border-b-0 sm:border-r"
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
                                    placeholder="输入快递单号后按 Enter 添加"
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
                            label="添加物流号"
                            aria-label="添加物流号"
                            variant="outline"
                            className="w-full shrink-0 sm:w-auto"
                            disabled={disabled}
                        >
                            <PlusIcon data-icon="inline-start" />
                            添加
                        </form.SubmitButton>
                    </form.AppForm>
                </div>
                <p className="text-xs text-muted-foreground">
                    按 Enter 或点击“添加”；可一次粘贴多个单号。
                </p>
            </form>
            <p className="text-xs leading-5 text-muted-foreground">
                同一商品可添加多个物流号；同一包裹包含多项商品时，请在对应商品下分别登记。
            </p>
        </div>
    )
}
