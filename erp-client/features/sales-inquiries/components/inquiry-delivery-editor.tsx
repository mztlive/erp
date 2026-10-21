"use client"

import { withForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { RegionCascadeSelect } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { demandFormDefaults } from "../lib/demand-form"
import { deliveryProgress, newAllocation } from "../lib/delivery-progress"
import type { InquiryLine, SalesInquiry } from "../types"

export const InquiryDeliveryEditor = withForm({
    defaultValues: demandFormDefaults,
    props: {
        index: 0,
        line: null as InquiryLine | null,
        inquiry: undefined as SalesInquiry | undefined,
        disabled: false,
    },
    render: function DeliveryEditor({ form, index, line, inquiry, disabled }) {
        if (!line) return null
        const prefix = `sales-inquiry-line-${toAutomationIdSegment(line.line_id)}`
        const progress = deliveryProgress(line)
        return (
            <fieldset disabled={disabled} className="min-w-0 space-y-6">
                <div className="rounded-lg border bg-muted/30 p-4">
                    <p className="text-sm font-medium">
                        需求{" "}
                        <QuantityValue
                            value={line.quantity}
                            unit={line.unit_code}
                        />
                    </p>
                    <p
                        className="mt-1 text-sm text-muted-foreground"
                        role="status"
                    >
                        {progress.label}
                    </p>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                    <form.Field
                        name={`draft.lines[${index}].fulfillment_kind`}
                        children={(field) => (
                            <div className="space-y-2">
                                <Label htmlFor={`${prefix}-fulfillment-kind`}>
                                    交付类型
                                </Label>
                                <OptionCombobox
                                    id={`${prefix}-fulfillment-kind`}
                                    aria-label="交付类型"
                                    allowClear={false}
                                    options={[
                                        {
                                            value: "physical",
                                            label: "实物配送",
                                        },
                                        {
                                            value: "virtual",
                                            label: "虚拟交付",
                                        },
                                        {
                                            value: "service",
                                            label: "服务交付",
                                        },
                                    ]}
                                    value={field.state.value}
                                    onValueChange={(value) =>
                                        field.handleChange(
                                            value as InquiryLine["fulfillment_kind"],
                                        )
                                    }
                                />
                            </div>
                        )}
                    />
                    <form.AppField
                        name={`draft.lines[${index}].delivery_terms`}
                        children={(field) => (
                            <field.TextField
                                id={`${prefix}-delivery-terms`}
                                label="交付条件"
                            />
                        )}
                    />
                </div>
                <form.Field
                    name={`draft.lines[${index}].allocations`}
                    mode="array"
                    children={(allocationsField) => (
                        <section className="space-y-3">
                            <div className="flex items-center justify-between gap-3">
                                <h4 className="text-sm font-medium">
                                    配送地址{" "}
                                    <span className="font-normal text-muted-foreground">
                                        （最多 100 个地址）
                                    </span>
                                </h4>
                                <Button
                                    id={`${prefix}-add-address`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        allocationsField.state.value.length >=
                                        100
                                    }
                                    onClick={() =>
                                        allocationsField.pushValue(
                                            newAllocation(line),
                                        )
                                    }
                                >
                                    添加地址
                                </Button>
                            </div>
                            {allocationsField.state.value.map(
                                (allocation, allocationIndex) => {
                                    const frozenAddress =
                                        inquiry?.requirements.some((snapshot) =>
                                            snapshot.data.lines.some((row) =>
                                                row.allocations.some(
                                                    (address) =>
                                                        address.allocation_id ===
                                                        allocation.allocation_id,
                                                ),
                                            ),
                                        ) ?? false
                                    const occupiedAddress =
                                        inquiry?.quantity_ledger.some(
                                            (row) =>
                                                row.state !== "released" &&
                                                row.selections.some(
                                                    (selection) =>
                                                        selection.allocation_id ===
                                                        allocation.allocation_id,
                                                ),
                                        ) ?? false
                                    const allocationPrefix = `${prefix}-address-${toAutomationIdSegment(allocation.allocation_id)}`
                                    return (
                                        <div
                                            key={allocation.allocation_id}
                                            className="grid gap-3 rounded-md bg-muted/40 p-3 sm:grid-cols-2"
                                        >
                                            {frozenAddress && (
                                                <div className="sm:col-span-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                                                    <span>
                                                        {occupiedAddress
                                                            ? "该地址已有销售占用；请先处理下游，或添加新地址分配未转数量。"
                                                            : "已提交的收件资料保留原记录，修改时请替换地址。"}
                                                    </span>
                                                    <Button
                                                        id={`${allocationPrefix}-replace`}
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={
                                                            occupiedAddress
                                                        }
                                                        onClick={() =>
                                                            allocationsField.handleChange(
                                                                allocationsField.state.value.map(
                                                                    (
                                                                        item,
                                                                        position,
                                                                    ) =>
                                                                        position ===
                                                                        allocationIndex
                                                                            ? {
                                                                                  ...item,
                                                                                  allocation_id:
                                                                                      crypto.randomUUID(),
                                                                              }
                                                                            : item,
                                                                ),
                                                            )
                                                        }
                                                    >
                                                        替换地址
                                                    </Button>
                                                </div>
                                            )}
                                            <form.AppField
                                                name={`draft.lines[${index}].allocations[${allocationIndex}].recipient`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${allocationPrefix}-recipient`}
                                                        disabled={frozenAddress}
                                                        label="收货人"
                                                    />
                                                )}
                                            />
                                            <form.AppField
                                                name={`draft.lines[${index}].allocations[${allocationIndex}].phone`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${allocationPrefix}-phone`}
                                                        disabled={frozenAddress}
                                                        label="联系电话"
                                                        type="tel"
                                                    />
                                                )}
                                            />
                                            <form.AppField
                                                name={`draft.lines[${index}].allocations[${allocationIndex}].quantity`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${allocationPrefix}-quantity`}
                                                        label="配送数量"
                                                        inputMode="decimal"
                                                    />
                                                )}
                                            />
                                            <div className="flex items-end justify-end">
                                                <Button
                                                    id={`${allocationPrefix}-remove`}
                                                    disabled={occupiedAddress}
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    onClick={() =>
                                                        allocationsField.removeValue(
                                                            allocationIndex,
                                                        )
                                                    }
                                                >
                                                    移除地址
                                                </Button>
                                            </div>
                                            <form.Field
                                                name={`draft.lines[${index}].allocations[${allocationIndex}].region_code`}
                                                children={(field) => (
                                                    <div className="space-y-2 sm:col-span-2">
                                                        <Label
                                                            htmlFor={`${allocationPrefix}-region-province`}
                                                        >
                                                            省 / 市 / 区县
                                                        </Label>
                                                        <RegionCascadeSelect
                                                            idPrefix={`${allocationPrefix}-region`}
                                                            disabled={
                                                                frozenAddress
                                                            }
                                                            leafOnly
                                                            value={
                                                                field.state
                                                                    .value
                                                            }
                                                            onValueChange={
                                                                field.handleChange
                                                            }
                                                        />
                                                    </div>
                                                )}
                                            />
                                            <form.AppField
                                                name={`draft.lines[${index}].allocations[${allocationIndex}].address`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${allocationPrefix}-detail`}
                                                        disabled={frozenAddress}
                                                        className="sm:col-span-2"
                                                        label="详细地址"
                                                    />
                                                )}
                                            />
                                        </div>
                                    )
                                },
                            )}
                        </section>
                    )}
                />

                {line.fulfillment_kind === "physical" &&
                    line.allocations.length === 0 && (
                        <p className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
                            尚未添加配送地址。首个地址默认分配全部需求数量。
                        </p>
                    )}
                <p className="text-xs leading-5 text-muted-foreground">
                    实物商品提交采购前须补齐收件资料，且地址分配数量须与需求数量一致。
                </p>
            </fieldset>
        )
    },
})
