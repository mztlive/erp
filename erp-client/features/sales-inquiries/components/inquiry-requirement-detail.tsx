import type { ReactNode } from "react"
import { TriangleAlertIcon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { deliveryAttention, deliveryProgress } from "../lib/delivery-progress"
import { inquiryDateInput } from "../lib/format"
import type { InquiryLine } from "../types"

export function InquiryRequirementDetail({
    line,
    unit,
    procurement,
    image,
    compact = false,
}: {
    line: InquiryLine
    unit: string
    procurement: boolean
    image: ReactNode
    compact?: boolean
}) {
    const progress = deliveryProgress({ ...line, unit_code: unit })
    const attention = deliveryAttention({ ...line, unit_code: unit })
    const showDemandQuantity =
        !compact ||
        line.fulfillment_kind !== "physical" ||
        line.allocations.length !== 1 ||
        progress.invalidQuantity ||
        progress.overAllocated ||
        progress.remaining !== "0"
    const dueDate = inquiryDateInput(line.delivery_due_at) || "未设置"

    return (
        <>
            {image}
            <dl
                className={
                    compact
                        ? "grid grid-cols-2 gap-x-4 gap-y-3 border-b border-border/60 pb-4"
                        : "grid grid-cols-2 gap-x-5 gap-y-4 border-b border-border/60 py-4"
                }
            >
                {showDemandQuantity && (
                    <div className="space-y-0.5">
                        <dt className="text-xs text-muted-foreground">
                            需求数量
                        </dt>
                        <dd>
                            <QuantityValue
                                value={line.quantity}
                                unit={unit}
                                className="text-body-compact font-semibold"
                            />
                        </dd>
                    </div>
                )}
                <div className="space-y-0.5">
                    <dt className="text-xs text-muted-foreground">
                        要求交付日
                    </dt>
                    <dd className="num text-body-compact font-medium">
                        {dueDate}
                    </dd>
                </div>
                {!procurement && line.sales_reference_price_gross && (
                    <div className="space-y-0.5">
                        <dt className="text-xs text-muted-foreground">
                            销售价
                        </dt>
                        <dd>
                            <MoneyValue
                                value={line.sales_reference_price_gross}
                                display="exact"
                                showHint={false}
                            />
                        </dd>
                    </div>
                )}
                {line.delivery_terms && (
                    <div className="col-span-2 space-y-0.5">
                        <dt className="text-xs text-muted-foreground">
                            交付要求
                        </dt>
                        <dd className="whitespace-pre-wrap text-body-compact wrap-anywhere">
                            {line.delivery_terms}
                        </dd>
                    </div>
                )}
            </dl>

            <section
                className={compact ? "space-y-2.5 pt-4" : "space-y-3 py-5"}
                aria-label="收货安排"
            >
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h3 className="text-body-compact font-medium">
                        {line.fulfillment_kind === "physical"
                            ? "配送地址"
                            : "交付安排"}
                    </h3>
                    {line.allocations.length > 0 && (
                        <span className="num text-xs text-muted-foreground">
                            共 {line.allocations.length} 个地址
                        </span>
                    )}
                </div>
                {attention && (
                    <Alert variant="warning" className="rounded-lg px-3 py-2">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertDescription>{attention}</AlertDescription>
                    </Alert>
                )}
                {line.allocations.length === 0 ? (
                    <p className="rounded-lg border border-dashed border-border/70 bg-surface-sunken/50 p-4 text-xs text-muted-foreground">
                        {line.fulfillment_kind === "physical"
                            ? "尚未填写收货地址。"
                            : `${progress.label}，无需实物收货地址。`}
                    </p>
                ) : (
                    <div className="space-y-2.5">
                        {line.allocations.map((allocation) => (
                            <article
                                key={allocation.allocation_id}
                                className="space-y-1.5 rounded-lg border border-border/60 bg-surface-sunken/60 p-3.5"
                            >
                                <div className="flex items-start justify-between gap-3">
                                    <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                                        <h4 className="font-medium text-foreground wrap-anywhere">
                                            {allocation.recipient ||
                                                "未填写收货人"}
                                        </h4>
                                        <span className="num text-xs text-muted-foreground wrap-anywhere">
                                            {allocation.phone || "未填写电话"}
                                        </span>
                                    </div>
                                    <QuantityValue
                                        value={allocation.quantity}
                                        unit={unit}
                                        className="shrink-0 font-medium"
                                    />
                                </div>
                                <p className="text-xs text-muted-foreground wrap-anywhere">
                                    {[
                                        allocation.region_name || "未填写地区",
                                        allocation.address || "未填写详细地址",
                                    ].join(" ")}
                                </p>
                            </article>
                        ))}
                    </div>
                )}
            </section>
        </>
    )
}
