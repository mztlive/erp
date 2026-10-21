"use client"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    InquirySalesBasis,
    InquirySalesSourceSnapshot,
} from "../api/inquiry-basis"
import { inquiryRowKey } from "../api/inquiry-basis"
import type { SalesDeliveryAllocation } from "../types"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"

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
    return (
        <form.Subscribe selector={(state) => state.values.lineItems}>
            {(lines) => (
                <section
                    className="space-y-4 border-t border-grid p-4 sm:p-5"
                    aria-labelledby="inquiry-sales-delivery-title"
                >
                    <h2
                        id="inquiry-sales-delivery-title"
                        className="font-heading font-semibold"
                    >
                        询价配送分配
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        每个地址的数量合计须等于商品开单数量；新增或变更收件资料请返回询价确认。
                    </p>
                    {lines.map((line, lineIndex) => (
                        <div
                            key={line.rowKey}
                            className="space-y-2 rounded-md border border-border p-3"
                        >
                            <p className="text-sm font-medium">
                                {line.name} ·{" "}
                                <QuantityValue
                                    value={line.quantity}
                                    unit={line.unit}
                                />
                            </p>
                            {(line.deliveryAllocations ?? []).length === 0 ? (
                                <p className="text-xs text-muted-foreground">
                                    按原询价交付要求办理，无实物配送地址。
                                </p>
                            ) : (
                                (line.deliveryAllocations ?? []).map(
                                    (delivery, addressIndex) => {
                                        const original = bindings(
                                            source,
                                            line.rowKey,
                                        ).find((row) =>
                                            matches(row.delivery, delivery),
                                        )
                                        if (!original)
                                            return (
                                                <p
                                                    key={`${line.rowKey}-unmatched`}
                                                    className="text-sm text-destructive"
                                                >
                                                    配送资料与来源不一致，请返回询价核对。
                                                </p>
                                            )
                                        const prefix = `inquiry-sales-delivery-${toAutomationIdSegment(line.rowKey)}-${toAutomationIdSegment(original.allocation_id)}`
                                        return (
                                            <div
                                                key={original.allocation_id}
                                                className="flex flex-wrap items-center gap-3 rounded-md bg-muted/30 p-3"
                                            >
                                                <div className="min-w-40 flex-1 text-sm">
                                                    <p>
                                                        {delivery.recipient} ·{" "}
                                                        {delivery.phone}
                                                    </p>
                                                    <p className="text-xs text-muted-foreground">
                                                        {delivery.region_name}
                                                        {delivery.address}
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
                                                                type="number"
                                                                step="any"
                                                                inputClassName="num"
                                                            />
                                                        )}
                                                    </form.AppField>
                                                </div>
                                                <Button
                                                    id={`${prefix}-remove`}
                                                    type="button"
                                                    size="sm"
                                                    variant="ghost"
                                                    onClick={() => {
                                                        form.setFieldValue(
                                                            `lineItems[${lineIndex}].deliveryAllocations`,
                                                            (
                                                                line.deliveryAllocations ??
                                                                []
                                                            ).filter(
                                                                (_, index) =>
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
                                )
                            )}
                        </div>
                    ))}
                </section>
            )}
        </form.Subscribe>
    )
}
