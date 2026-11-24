"use client"

import Link from "next/link"
import { ArrowUpRightIcon, ChevronRightIcon, InfoIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { paymentReminder } from "../../lib/validation"
import type { FulfillmentOperation } from "../../types"

/** 付款事实以金额标签放入任务标题，不把审批状态当成已支付。 */
export function FulfillmentPaymentBadges({
    operation,
}: {
    operation: FulfillmentOperation
}) {
    return (
        <>
            {operation.gate.requiredAmount != null && (
                <Badge variant="outline">
                    约定预付{" "}
                    <MoneyValue value={operation.gate.requiredAmount} />
                </Badge>
            )}
            {operation.gate.effectivePaidAmount != null && (
                <Badge variant="outline">
                    有效已付{" "}
                    <MoneyValue value={operation.gate.effectivePaidAmount} />
                </Badge>
            )}
        </>
    )
}

/** 先展示实际付款缺口；跳转只读采购票款资料，不直接新建付款申请。 */
export function FulfillmentPaymentNotice({
    operation,
    showAmounts = true,
}: {
    operation: FulfillmentOperation
    showAmounts?: boolean
}) {
    return (
        <Collapsible
            id="prepayment-gate"
            role="note"
            className="border-t border-border/50 pt-3 text-xs text-muted-foreground"
        >
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <p className="flex min-w-0 items-center gap-2 leading-6 text-foreground">
                    <InfoIcon
                        aria-hidden="true"
                        className="size-4 shrink-0 text-muted-foreground"
                    />
                    <span>{paymentReminder(operation)}</span>
                </p>
                {showAmounts && (
                    <CollapsibleTrigger
                        id="fulfillment-payment-details-toggle"
                        className="group"
                        render={
                            <Button
                                variant="ghost"
                                size="sm"
                                className="text-xs font-normal text-muted-foreground"
                            />
                        }
                    >
                        <ChevronRightIcon
                            aria-hidden="true"
                            className="size-3 transition-transform group-aria-expanded:rotate-90"
                        />
                        付款明细
                    </CollapsibleTrigger>
                )}
                <FulfillmentPaymentLink operation={operation} />
            </div>
            {showAmounts && (
                <CollapsibleContent>
                    <div className="flex flex-wrap items-center gap-x-5 gap-y-1 pt-2 pl-6 leading-6">
                        {operation.gate.requiredAmount != null && (
                            <span>
                                约定预付{" "}
                                <MoneyValue
                                    value={operation.gate.requiredAmount}
                                />
                            </span>
                        )}
                        {operation.gate.effectivePaidAmount != null && (
                            <span>
                                有效已付{" "}
                                <MoneyValue
                                    value={operation.gate.effectivePaidAmount}
                                />
                            </span>
                        )}
                        <span>
                            付款审批通过不代表已支付。请按实际发货或交付情况确认履约。
                        </span>
                    </div>
                </CollapsibleContent>
            )}
        </Collapsible>
    )
}

/** 采购付款入口复用采购单查看权限，可独立放入办理页标题操作区。 */
export function FulfillmentPaymentLink({
    operation,
    id = "fulfillment-operations-gate-view-payment",
}: {
    operation: FulfillmentOperation
    id?: string
}) {
    const profile = useAccountProfileQuery()
    const purchaseOrderId = operation.source.purchaseOrderId
    const canViewPayment =
        purchaseOrderId &&
        hasPermission(profile.data?.permissions, "purchase_order:detail")

    if (!canViewPayment) return null
    return (
        <Button
            id={id}
            variant="ghost"
            size="sm"
            render={
                <Link
                    href={`/procurement/orders/${encodeURIComponent(purchaseOrderId)}?section=payable`}
                />
            }
        >
            查看采购付款{" "}
            <ArrowUpRightIcon aria-hidden="true" data-icon="inline-end" />
        </Button>
    )
}
