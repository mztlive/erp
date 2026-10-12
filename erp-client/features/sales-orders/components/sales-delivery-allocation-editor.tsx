"use client"

import { Button } from "@/components/ui/button"
import { toFieldErrors } from "@/components/form"
import { FieldError } from "@/components/ui/field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"

/** 每条商品独立分配地址，选品册按份展开的商品使用相同数量规则。 */
export function SalesDeliveryAllocationEditor({
    form,
}: {
    form: SalesOrderCreateFormApi
}) {
    return (
        <form.Subscribe
            selector={(state) => ({
                nature: state.values.nature,
                lines: state.values.lineItems,
            })}
        >
            {({ nature, lines }) =>
                nature !== "physical_service" || !lines.length ? null : (
                    <section
                        className="space-y-4 rounded-lg border border-border p-4"
                        aria-label="分地址交付"
                    >
                        <div>
                            <h3 className="font-medium">分地址交付</h3>
                            <p className="mt-1 text-xs text-muted-foreground">
                                按商品填写每个地址的数量，合计须等于该商品数量。选品册按份采购的商品同样可分配；未指定时可在履约环节登记。
                            </p>
                        </div>
                        {lines.map((line, rowIndex) => {
                            const prefix = `sales-delivery-${toAutomationIdSegment(line.rowKey)}`
                            const allocations = line.deliveryAllocations ?? []
                            let summary = "尚未指定地址"
                            try {
                                const total = sumFixed(
                                    allocations.map(
                                        (row) => row.quantity || "0",
                                    ),
                                    { maxScale: 6, outputScale: 6 },
                                )
                                summary = `已分配 ${total} / ${line.quantity} ${line.unit}`
                                if (
                                    allocations.length &&
                                    compareDecimal(total, line.quantity, 6) !==
                                        0
                                )
                                    summary += "，请核对合计"
                            } catch {
                                summary = "请填写有效分配数量"
                            }
                            return (
                                <div
                                    key={line.rowKey}
                                    className="space-y-3 border-t border-border pt-3"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <div className="text-sm">
                                            <span className="font-medium">
                                                {line.name ||
                                                    `商品 ${rowIndex + 1}`}
                                            </span>
                                            <span className="ml-3 text-muted-foreground">
                                                {summary}
                                            </span>
                                        </div>
                                        <Button
                                            id={`${prefix}-add`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={allocations.length >= 100}
                                            onClick={() =>
                                                form.setFieldValue(
                                                    `lineItems[${rowIndex}].deliveryAllocations`,
                                                    [
                                                        ...allocations,
                                                        {
                                                            recipient: "",
                                                            phone: "",
                                                            address: "",
                                                            quantity:
                                                                allocations.length
                                                                    ? ""
                                                                    : line.quantity,
                                                        },
                                                    ],
                                                )
                                            }
                                        >
                                            添加收货地址
                                        </Button>
                                    </div>
                                    {allocations.map((_, allocationIndex) => (
                                        <div
                                            key={allocationIndex}
                                            className="grid gap-3 rounded-md bg-muted/30 p-3 sm:grid-cols-2 lg:grid-cols-[1fr_1.2fr_2fr_1fr_auto]"
                                        >
                                            {(
                                                [
                                                    ["recipient", "收货人"],
                                                    ["phone", "联系电话"],
                                                    ["address", "完整收货地址"],
                                                    [
                                                        "quantity",
                                                        `数量（${line.unit || "基础单位"}）`,
                                                    ],
                                                ] as const
                                            ).map(([name, label]) => (
                                                <form.AppField
                                                    key={name}
                                                    name={`lineItems[${rowIndex}].deliveryAllocations[${allocationIndex}].${name}`}
                                                >
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-${allocationIndex}-${name}`}
                                                            label={label}
                                                            step="any"
                                                            type={
                                                                name ===
                                                                "quantity"
                                                                    ? "number"
                                                                    : "text"
                                                            }
                                                        />
                                                    )}
                                                </form.AppField>
                                            ))}
                                            <Button
                                                id={`${prefix}-${allocationIndex}-remove`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className="self-end"
                                                onClick={() =>
                                                    form.setFieldValue(
                                                        `lineItems[${rowIndex}].deliveryAllocations`,
                                                        allocations.filter(
                                                            (_, index) =>
                                                                index !==
                                                                allocationIndex,
                                                        ),
                                                    )
                                                }
                                            >
                                                移除
                                            </Button>
                                        </div>
                                    ))}
                                    <form.AppField
                                        name={`lineItems[${rowIndex}].deliveryAllocations`}
                                    >
                                        {(field) => (
                                            <FieldError
                                                errors={toFieldErrors(
                                                    field.state.meta.errors,
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            )
                        })}
                    </section>
                )
            }
        </form.Subscribe>
    )
}
