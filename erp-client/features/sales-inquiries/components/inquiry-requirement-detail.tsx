import type { ReactNode } from "react"
import { TriangleAlertIcon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { deliveryAttention, deliveryProgress } from "../lib/delivery-progress"
import { inquiryDate } from "../lib/format"
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
    return (
        <>
            {image}
            <section
                className={compact ? "space-y-3" : "space-y-3 py-5"}
                aria-label="收货安排"
            >
                <div className="flex flex-wrap items-center gap-3">
                    <h3 className="font-medium">
                        {line.fulfillment_kind === "physical"
                            ? "配送地址"
                            : "交付安排"}
                    </h3>
                    {line.allocations.length > 0 && (
                        <span className="text-body-compact text-muted-foreground">
                            {line.allocations.length} 个地址
                        </span>
                    )}
                </div>
                {attention && (
                    <Alert variant="warning" className="rounded-lg px-3 py-2">
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertDescription>
                            {attention}。请核对收货明细。
                        </AlertDescription>
                    </Alert>
                )}
                {line.allocations.length === 0 ? (
                    <p className="rounded-lg bg-muted/40 p-4 text-xs text-muted-foreground">
                        {line.fulfillment_kind === "physical"
                            ? "尚未填写收货地址。"
                            : `${progress.label}，无需实物收货地址。`}
                    </p>
                ) : (
                    line.allocations.map((allocation) => (
                        <article
                            key={allocation.allocation_id}
                            className="space-y-3 rounded-lg bg-muted/40 p-4"
                        >
                            <div className="space-y-1">
                                <h4 className="font-medium wrap-anywhere">
                                    {allocation.recipient || "未填写收货人"}
                                </h4>
                                <p className="num text-xs text-muted-foreground wrap-anywhere">
                                    {allocation.phone || "未填写联系电话"}
                                </p>
                            </div>
                            <p className="whitespace-pre-wrap text-body-compact leading-6 text-muted-foreground wrap-anywhere">
                                {allocation.region_name || "未填写地区"}
                                <br />
                                {allocation.address || "未填写详细地址"}
                            </p>
                            <div className="flex flex-wrap items-baseline justify-between gap-2 border-t pt-3 text-body-compact">
                                <span className="text-muted-foreground">
                                    分配数量
                                </span>
                                <QuantityValue
                                    value={allocation.quantity}
                                    unit={unit}
                                />
                            </div>
                        </article>
                    ))
                )}
            </section>
            <dl className={compact ? "space-y-4" : "space-y-5 border-b py-5"}>
                {showDemandQuantity && (
                    <div className="space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            需求数量
                        </dt>
                        <dd>
                            <QuantityValue
                                value={line.quantity}
                                unit={unit}
                                className={
                                    compact
                                        ? "text-body-compact font-medium"
                                        : "text-value-compact font-semibold tracking-tight"
                                }
                            />
                        </dd>
                    </div>
                )}
                <div className="space-y-1">
                    <dt className="text-xs text-muted-foreground">要求交付</dt>
                    <dd
                        className={
                            compact
                                ? "num text-body-compact font-medium"
                                : "num text-body-compact font-semibold"
                        }
                    >
                        {inquiryDate(line.delivery_due_at)}
                    </dd>
                </div>
                {line.delivery_terms && (
                    <div className="space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            交付要求
                        </dt>
                        <dd className="whitespace-pre-wrap text-body-compact wrap-anywhere">
                            {line.delivery_terms}
                        </dd>
                    </div>
                )}
                {!procurement && line.sales_reference_price_gross && (
                    <div className="space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            销售价
                        </dt>
                        <dd>
                            <MoneyValue
                                value={line.sales_reference_price_gross}
                            />
                        </dd>
                    </div>
                )}
            </dl>
        </>
    )
}
