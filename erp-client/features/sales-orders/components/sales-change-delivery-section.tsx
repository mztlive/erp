"use client"
import { PlusIcon } from "lucide-react"
import { toFieldErrors } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed } from "@/lib/fixed-decimal"
import { allocationSummary } from "./sales-delivery-allocation-editor"
import { ChangeOrderSection } from "@/components/business/change-order-section"
import {
    salesChangeLineRemoved,
    type SalesChangeDraftValues,
} from "../lib/sales-change-draft"
import type { SalesChangeDraftFormApi } from "../lib/sales-change-draft-form-types"

type LineValues = SalesChangeDraftValues["lines"][number]

const ADDRESS_GRID =
    "grid gap-3 sm:grid-cols-2 @3xl/delivery:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)_minmax(0,2.6fr)_7rem_3.5rem]"

/** 交付安排：按商品填写交付期限、服务地区和分地址数量。 */
export function SalesChangeDeliverySection({
    form,
    id,
    disabled,
}: {
    form: SalesChangeDraftFormApi
    id: string
    disabled: boolean
}) {
    return (
        <form.Subscribe selector={(state) => state.values.lines}>
            {(lines) => {
                const rows = lines.flatMap((line, index) =>
                    line.source.goods &&
                    (!salesChangeLineRemoved(line) ||
                        line.deliveryAllocations.length)
                        ? [{ line, index }]
                        : [],
                )
                if (!rows.length) return null
                return (
                    <ChangeOrderSection
                        id={`${id}-delivery`}
                        title="交付安排"
                        description="每项商品须有交付期限。分地址选填，填写后各地址数量合计须等于商品数量；也可在履约时补充。"
                    >
                        <div className="@container/delivery min-w-0 divide-y divide-grid rounded-lg border border-grid">
                            {rows.map(({ line, index }) => (
                                <DeliveryGroup
                                    key={line.source.sales_order_line_id}
                                    form={form}
                                    id={id}
                                    line={line}
                                    index={index}
                                    disabled={disabled}
                                />
                            ))}
                        </div>
                    </ChangeOrderSection>
                )
            }}
        </form.Subscribe>
    )
}

function DeliveryGroup({
    form,
    id,
    line,
    index,
    disabled,
}: {
    form: SalesChangeDraftFormApi
    id: string
    line: LineValues
    index: number
    disabled: boolean
}) {
    const source = line.source
    const unit = source.unit_snapshot ?? ""
    const removed = salesChangeLineRemoved(line)
    const allocations = line.deliveryAllocations
    const summary = allocationSummary({ ...line, unit })
    const prefix = `${id}-${source.line_no}`
    const labels = [
        ["recipient", "收货人"],
        ["phone", "联系电话"],
        ["address", "完整收货地址"],
        ["quantity", `数量（${unit || "基础单位"}）`],
    ] as const
    return (
        <div className="min-w-0 space-y-4 p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1.5">
                    <p className="break-words text-sm font-medium">
                        {source.line_no}. {source.item_name_snapshot}
                        <span className="num ml-2 font-normal text-muted-foreground">
                            {removed
                                ? "已移除"
                                : `${compactFixed(line.quantity || "0")} ${unit}`}
                        </span>
                    </p>
                    <div
                        className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        {removed ? (
                            <Badge variant="warning">
                                商品已移除，请移除下方收货地址
                            </Badge>
                        ) : (
                            <>
                                <span className="num">{summary.summary}</span>
                                {summary.status ? (
                                    <Badge
                                        variant={
                                            summary.complete
                                                ? "success"
                                                : "warning"
                                        }
                                    >
                                        {summary.status}
                                    </Badge>
                                ) : null}
                            </>
                        )}
                    </div>
                </div>
                <Button
                    id={`${prefix}-address-add`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={disabled || removed || allocations.length >= 100}
                    onClick={() =>
                        form.setFieldValue(
                            `lines[${index}].deliveryAllocations`,
                            [
                                ...allocations,
                                {
                                    rowKey: crypto.randomUUID(),
                                    recipient: "",
                                    phone: "",
                                    address: "",
                                    quantity: allocations.length
                                        ? ""
                                        : line.quantity,
                                },
                            ],
                        )
                    }
                >
                    <PlusIcon aria-hidden="true" />
                    添加收货地址
                </Button>
            </div>
            {removed ? null : (
                <div className="grid gap-3 sm:grid-cols-[minmax(0,14rem)_minmax(0,20rem)]">
                    <form.AppField name={`lines[${index}].dueDate`}>
                        {(field) => (
                            <field.TextField
                                id={`${prefix}-due`}
                                type="date"
                                label="交付期限"
                                required
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name={`lines[${index}].serviceRegion`}>
                        {(field) => (
                            <field.TextField
                                id={`${prefix}-region`}
                                label="服务地区"
                                placeholder="选填"
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                </div>
            )}
            {allocations.length ? (
                <div>
                    <div
                        aria-hidden="true"
                        className={`${ADDRESS_GRID} hidden rounded-t-md border-b border-grid bg-muted/40 px-3 py-2 text-xs text-muted-foreground @3xl/delivery:grid`}
                    >
                        {labels.map(([name, label]) => (
                            <span key={name}>{label}</span>
                        ))}
                        <span className="text-center">操作</span>
                    </div>
                    <div className="divide-y divide-grid">
                        {allocations.map((row, addressIndex) => {
                            const rowId = `${prefix}-${toAutomationIdSegment(row.rowKey)}`
                            return (
                                <div
                                    key={row.rowKey}
                                    className={`${ADDRESS_GRID} items-start py-3 @3xl/delivery:px-3`}
                                >
                                    {labels.map(([name, label]) => (
                                        <form.AppField
                                            key={name}
                                            name={`lines[${index}].deliveryAllocations[${addressIndex}].${name}`}
                                        >
                                            {(field) => (
                                                <field.TextField
                                                    id={`${rowId}-${name}`}
                                                    label={label}
                                                    disabled={disabled}
                                                    className={`min-w-0 gap-2 @3xl/delivery:[&>[data-slot=field-label]]:sr-only ${name === "address" ? "sm:col-span-2 @3xl/delivery:col-span-1" : ""}`}
                                                    inputClassName={
                                                        name === "quantity"
                                                            ? "num"
                                                            : undefined
                                                    }
                                                    inputMode={
                                                        name === "quantity"
                                                            ? "decimal"
                                                            : undefined
                                                    }
                                                    placeholder={
                                                        name === "address"
                                                            ? "省、市、区及详细地址"
                                                            : undefined
                                                    }
                                                    type={
                                                        name === "phone"
                                                            ? "tel"
                                                            : "text"
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                    ))}
                                    <Button
                                        id={`${rowId}-address-remove`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        aria-label={`移除${source.item_name_snapshot}的第 ${addressIndex + 1} 个收货地址`}
                                        className="self-end justify-self-end text-muted-foreground @3xl/delivery:mt-0.5 @3xl/delivery:self-start"
                                        disabled={disabled}
                                        onClick={() =>
                                            form.setFieldValue(
                                                `lines[${index}].deliveryAllocations`,
                                                allocations.filter(
                                                    (_, rowIndex) =>
                                                        rowIndex !==
                                                        addressIndex,
                                                ),
                                            )
                                        }
                                    >
                                        移除
                                    </Button>
                                </div>
                            )
                        })}
                    </div>
                </div>
            ) : null}
            <form.AppField name={`lines[${index}].deliveryAllocations`}>
                {(field) => (
                    <FieldError
                        errors={toFieldErrors(field.state.meta.errors)}
                    />
                )}
            </form.AppField>
        </div>
    )
}
