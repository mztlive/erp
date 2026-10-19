"use client"

import { useRef, useState } from "react"
import { PlusIcon } from "lucide-react"
import { z } from "zod"
import { QuantityValue } from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { FieldError } from "@/components/ui/field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { useDeliveryCoverage } from "../hooks/use-delivery-coverage"
import { allocationSummary } from "../lib/sales-delivery-allocation-summary"
import {
    deliveryAllocationsSchema,
    type CreateSalesOrderFormValues,
} from "../lib/sales-order-create-validation"
import type { SalesDeliveryAllocation } from "../types"
import { DeliveryCoverageHint } from "./delivery-coverage-hint"
import { DeliveryRegionField } from "./delivery-region-field"

type DeliveryLine = CreateSalesOrderFormValues["lineItems"][number]
type Props = {
    line: DeliveryLine
    onApply: (rows: SalesDeliveryAllocation[]) => void
}

export function SalesDeliveryAllocationDialog({ line, onApply }: Props) {
    const [open, setOpen] = useState(false)
    const prefix = `sales-delivery-${toAutomationIdSegment(line.rowKey)}`
    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger
                id={`${prefix}-edit`}
                render={<Button type="button" variant="ghost" size="sm" />}
            >
                {line.deliveryAllocations?.length ? "修改" : "设置地址"}
            </DialogTrigger>
            {open ? (
                <DeliveryAllocationForm
                    line={line}
                    onApply={onApply}
                    onClose={() => setOpen(false)}
                    prefix={prefix}
                />
            ) : null}
        </Dialog>
    )
}

/** 弹窗内独立编辑；只有校验通过并确认后才写回销售单。 */
function DeliveryAllocationForm({
    line,
    onApply,
    onClose,
    prefix,
}: Props & { onClose: () => void; prefix: string }) {
    const sequence = useRef(0)
    const schema = z
        .object({
            rows: deliveryAllocationsSchema
                .unwrap()
                .element.extend({ editKey: z.string() })
                .array()
                .max(100, "每条商品最多分配 100 个地址"),
        })
        .superRefine(({ rows }, context) => {
            if (!rows.length) return
            const summary = allocationSummary({
                ...line,
                deliveryAllocations: rows,
            })
            if (!summary.complete)
                context.addIssue({
                    code: "custom",
                    path: ["rows"],
                    message: summary.status,
                })
        })
    const initialRows = useRef<
        (SalesDeliveryAllocation & { editKey: string })[] | null
    >(null)
    if (!initialRows.current) {
        const saved = line.deliveryAllocations?.length
            ? line.deliveryAllocations
            : [
                  {
                      recipient: "",
                      phone: "",
                      region_code: "",
                      address: "",
                      quantity: line.quantity,
                  },
              ]
        initialRows.current = saved.map((row) => ({
            ...row,
            editKey: `address-${++sequence.current}`,
        }))
    }
    const form = useAppForm({
        defaultValues: { rows: initialRows.current },
        validators: { onChange: schema },
        onSubmit: ({ value }) => {
            onApply(
                value.rows.map((row) => ({
                    recipient: row.recipient.trim(),
                    phone: row.phone.trim(),
                    region_code: row.region_code,
                    address: row.address.trim(),
                    quantity: row.quantity.trim(),
                })),
            )
            onClose()
        },
    })
    return (
        <DialogContent
            className="flex max-h-[85dvh] flex-col sm:max-w-2xl"
            closeButtonId={`${prefix}-close`}
        >
            <DialogHeader>
                <DialogTitle>设置收货地址</DialogTitle>
                <DialogDescription className="break-words pr-6">
                    {line.name || "商品"}
                    {line.specification ? ` · ${line.specification}` : ""} ·
                    销售数量{" "}
                    <QuantityValue value={line.quantity} unit={line.unit} />
                </DialogDescription>
            </DialogHeader>
            <form
                className="flex min-h-0 flex-col gap-4"
                noValidate
                onSubmit={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    void form.handleSubmit()
                }}
            >
                <form.Subscribe selector={(state) => state.values.rows}>
                    {(rows) => {
                        const summary = allocationSummary({
                            ...line,
                            deliveryAllocations: rows,
                        })
                        return (
                            <div
                                className="space-y-1 rounded-lg bg-muted/40 px-3 py-2 text-sm"
                                aria-live="polite"
                            >
                                <p className="num">
                                    {rows.length
                                        ? summary.summary
                                        : "暂不设置收货地址"}
                                </p>
                                <p
                                    className={`text-xs ${rows.length && !summary.complete ? "text-warning-soft-foreground" : "text-muted-foreground"}`}
                                >
                                    {rows.length
                                        ? summary.status
                                        : "保存后清除该商品的地址，可在履约前补充。"}
                                </p>
                            </div>
                        )
                    }}
                </form.Subscribe>
                <div className="min-h-0 space-y-4 overflow-y-auto pr-1">
                    <form.AppField name="rows" mode="array">
                        {(arrayField) => (
                            <div className="space-y-4">
                                {arrayField.state.value.map((row, index) => (
                                    <div
                                        key={row.editKey}
                                        className="space-y-3 rounded-lg border border-grid p-4"
                                    >
                                        <div className="flex items-center justify-between gap-2">
                                            <h3 className="text-sm font-medium">
                                                地址 {index + 1}
                                            </h3>
                                            <Button
                                                id={`${prefix}-${row.editKey}-remove`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-label={`移除第 ${index + 1} 个收货地址`}
                                                onClick={() =>
                                                    arrayField.removeValue(
                                                        index,
                                                    )
                                                }
                                            >
                                                移除
                                            </Button>
                                        </div>
                                        <div className="grid gap-3 sm:grid-cols-2">
                                            <form.AppField
                                                name={`rows[${index}].recipient`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-${row.editKey}-recipient`}
                                                        label="收货人"
                                                        required
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`rows[${index}].phone`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-${row.editKey}-phone`}
                                                        label="联系电话"
                                                        type="tel"
                                                        required
                                                    />
                                                )}
                                            </form.AppField>
                                        </div>
                                        <form.AppField
                                            name={`rows[${index}].region_code`}
                                        >
                                            {(field) => (
                                                <DeliveryRegionField
                                                    id={`${prefix}-${row.editKey}-region`}
                                                    required
                                                    value={field.state.value}
                                                    onValueChange={(code) =>
                                                        field.handleChange(code)
                                                    }
                                                    onBlur={field.handleBlur}
                                                    invalid={
                                                        field.state.meta
                                                            .isTouched &&
                                                        !field.state.meta
                                                            .isValid
                                                    }
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                    hint={
                                                        <DraftCoverageHint
                                                            sku={line.sku}
                                                            regionCode={
                                                                field.state
                                                                    .value
                                                            }
                                                            id={`${prefix}-${row.editKey}-coverage`}
                                                        />
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField
                                            name={`rows[${index}].address`}
                                        >
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-${row.editKey}-address`}
                                                    label="详细地址"
                                                    placeholder="街道、门牌号"
                                                    required
                                                />
                                            )}
                                        </form.AppField>
                                        <form.AppField
                                            name={`rows[${index}].quantity`}
                                        >
                                            {(field) => (
                                                <field.TextField
                                                    id={`${prefix}-${row.editKey}-quantity`}
                                                    label={`分配数量（${line.unit || "基础单位"}）`}
                                                    type="number"
                                                    step="any"
                                                    inputClassName="num"
                                                    className="sm:max-w-48"
                                                    required
                                                />
                                            )}
                                        </form.AppField>
                                    </div>
                                ))}
                                <FieldError
                                    errors={toFieldErrors(
                                        arrayField.state.meta.errors,
                                    )}
                                />
                                <Button
                                    id={`${prefix}-add`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={
                                        arrayField.state.value.length >= 100
                                    }
                                    onClick={() => {
                                        let remaining = ""
                                        try {
                                            const total = sumFixed(
                                                arrayField.state.value.map(
                                                    (row) =>
                                                        row.quantity || "0",
                                                ),
                                                { maxScale: 6, outputScale: 6 },
                                            )
                                            if (
                                                compareDecimal(
                                                    total,
                                                    line.quantity,
                                                    6,
                                                ) < 0
                                            )
                                                remaining = subtractFixed(
                                                    line.quantity,
                                                    total,
                                                    {
                                                        maxScale: 6,
                                                        outputScale: 6,
                                                    },
                                                )
                                        } catch {
                                            /* 尚未填完数量时保留空值，由用户分配。 */
                                        }
                                        arrayField.pushValue({
                                            recipient: "",
                                            phone: "",
                                            region_code: "",
                                            address: "",
                                            quantity: remaining,
                                            editKey: `address-${++sequence.current}`,
                                        })
                                    }}
                                >
                                    <PlusIcon aria-hidden="true" />
                                    添加收货地址
                                </Button>
                            </div>
                        )}
                    </form.AppField>
                </div>
                <DialogFooter>
                    <Button
                        id={`${prefix}-cancel`}
                        type="button"
                        variant="ghost"
                        onClick={onClose}
                    >
                        取消
                    </Button>
                    <form.AppForm>
                        <form.SubmitButton
                            id={`${prefix}-apply`}
                            label="保存收货安排"
                        />
                    </form.AppForm>
                </DialogFooter>
            </form>
        </DialogContent>
    )
}

function DraftCoverageHint({
    sku,
    regionCode,
    id,
}: {
    sku: string
    regionCode: string
    id: string
}) {
    const coverageHint = useDeliveryCoverage([
        { sku, deliveryAllocations: [{ region_code: regionCode }] },
    ])
    return <DeliveryCoverageHint id={id} hint={coverageHint(sku, regionCode)} />
}
