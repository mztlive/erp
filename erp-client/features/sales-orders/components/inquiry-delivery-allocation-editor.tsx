"use client"

import { useStore } from "@tanstack/react-form"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import {
    DetailWorkspaceSection,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    InquirySalesBasis,
    InquirySalesSourceSnapshot,
} from "../api/inquiry-basis"
import { inquiryRowKey } from "../api/inquiry-basis"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import type { SalesDeliveryAllocation } from "../types"

type Source = InquirySalesBasis | InquirySalesSourceSnapshot
type Binding = { allocation_id: string; delivery: SalesDeliveryAllocation }

function bindings(source: Source, rowKey: string): Binding[] {
    if ("lineItems" in source) {
        const line = source.lines.find(
            (row) => inquiryRowKey(row.selection) === rowKey,
        )
        return (
            line?.selection.allocations.flatMap((selected) => {
                const original = line.requirement.allocations.find(
                    (row) => row.allocation_id === selected.allocation_id,
                )
                return original
                    ? [
                          {
                              allocation_id: selected.allocation_id,
                              delivery: original,
                          },
                      ]
                    : []
            }) ?? []
        )
    }
    return (
        source.lines.find((line) => line.sales_order_line_id === rowKey)
            ?.allocations ?? []
    )
}

function matches(
    left: SalesDeliveryAllocation,
    right: SalesDeliveryAllocation,
) {
    return (
        left.recipient === right.recipient &&
        left.phone === right.phone &&
        left.region_code === right.region_code &&
        left.address === right.address
    )
}

/** 询价地址身份固定；销售只调整本次承担数量或移除不转的地址。 */
export function InquiryDeliveryAllocationEditor({
    form,
    source,
}: {
    form: SalesOrderCreateFormApi
    source: Source
}) {
    const lines = useStore(form.store, (state) => state.values.lineItems)
    const { nameForUnit } = useProductUnitNames(
        lines.map((line) => line.sku),
        lines.length > 0,
    )
    return (
        <form.Subscribe selector={(state) => state.values.lineItems}>
            {(lines) => (
                <DetailWorkspaceSection
                    title="询价配送分配"
                    headingId="inquiry-sales-delivery-title"
                    width="form"
                    subtitle="各地址数量合计须等于商品开单数量；变更收件资料请返回询价确认"
                >
                    <div
                        data-slot="detail-form-grid"
                        className="divide-y divide-border/60 border-y border-border/60"
                    >
                        {lines.map((line, lineIndex) => (
                            <div
                                key={line.rowKey}
                                className="space-y-3 py-4 first:pt-3.5 last:pb-3.5"
                            >
                                <div className="flex flex-wrap items-baseline justify-between gap-2">
                                    <p className="font-medium text-foreground">
                                        {line.name}
                                    </p>
                                    <span className="text-xs text-muted-foreground">
                                        开单数量{" "}
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={nameForUnit(
                                                line.unit,
                                                line.sku,
                                            )}
                                        />
                                    </span>
                                </div>
                                {(line.deliveryAllocations ?? []).length ===
                                0 ? (
                                    <p className="text-xs text-muted-foreground">
                                        按原询价交付要求办理，无实物配送地址。
                                    </p>
                                ) : (
                                    <div className="space-y-2">
                                        {(line.deliveryAllocations ?? []).map(
                                            (delivery, addressIndex) => {
                                                const original = bindings(
                                                    source,
                                                    line.rowKey,
                                                ).find((row) =>
                                                    matches(
                                                        row.delivery,
                                                        delivery,
                                                    ),
                                                )
                                                if (!original)
                                                    return (
                                                        <p
                                                            key={`${line.rowKey}-unmatched`}
                                                            className="text-destructive"
                                                        >
                                                            配送资料与来源不一致，请返回询价核对。
                                                        </p>
                                                    )
                                                const prefix = `inquiry-sales-delivery-${toAutomationIdSegment(line.rowKey)}-${toAutomationIdSegment(original.allocation_id)}`
                                                return (
                                                    <div
                                                        key={
                                                            original.allocation_id
                                                        }
                                                        className="flex flex-wrap items-center gap-3 rounded-lg bg-surface-sunken/80 px-3.5 py-2.5"
                                                    >
                                                        <div className="min-w-44 flex-1">
                                                            <p className="font-medium text-foreground">
                                                                {
                                                                    delivery.recipient
                                                                }
                                                                <span className="num ml-2 font-normal text-muted-foreground">
                                                                    {
                                                                        delivery.phone
                                                                    }
                                                                </span>
                                                            </p>
                                                            <p className="mt-0.5 text-xs text-muted-foreground">
                                                                {
                                                                    delivery.region_name
                                                                }
                                                                {
                                                                    delivery.address
                                                                }
                                                            </p>
                                                        </div>
                                                        <div className="w-28">
                                                            <form.AppField
                                                                name={`lineItems[${lineIndex}].deliveryAllocations[${addressIndex}].quantity`}
                                                            >
                                                                {(field) => (
                                                                    <field.TextField
                                                                        id={`${prefix}-quantity`}
                                                                        label="本次分配数量"
                                                                        type="text"
                                                                        inputMode="decimal"
                                                                        inputClassName="num text-right"
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </div>
                                                        <Button
                                                            id={`${prefix}-remove`}
                                                            type="button"
                                                            size="sm"
                                                            variant="ghost"
                                                            className={
                                                                detailWorkspaceStyles.quietDangerButton
                                                            }
                                                            onClick={() => {
                                                                form.setFieldValue(
                                                                    `lineItems[${lineIndex}].deliveryAllocations`,
                                                                    (
                                                                        line.deliveryAllocations ??
                                                                        []
                                                                    ).filter(
                                                                        (
                                                                            _,
                                                                            index,
                                                                        ) =>
                                                                            index !==
                                                                            addressIndex,
                                                                    ),
                                                                )
                                                            }}
                                                        >
                                                            移除此地址
                                                        </Button>
                                                    </div>
                                                )
                                            },
                                        )}
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                </DetailWorkspaceSection>
            )}
        </form.Subscribe>
    )
}
