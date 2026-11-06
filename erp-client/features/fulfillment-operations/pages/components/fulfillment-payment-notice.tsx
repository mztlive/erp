"use client"

import Link from "next/link"
import { ArrowUpRightIcon, ChevronRightIcon, InfoIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { paymentReminder } from "../../lib/validation"
import type { FulfillmentOperation } from "../../types"

/** 先展示实际付款缺口；跳转只读采购票款资料，不直接新建付款申请。 */
export function FulfillmentPaymentNotice({
    operation,
}: {
    operation: FulfillmentOperation
}) {
    const profile = useAccountProfileQuery()
    const purchaseOrderId = operation.source.purchaseOrderId
    const canViewPayment =
        purchaseOrderId &&
        hasPermission(profile.data?.permissions, "purchase_order:detail")

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
                {canViewPayment && (
                    <Button
                        id="fulfillment-operations-gate-view-payment"
                        variant="link"
                        size="sm"
                        className="text-xs"
                        render={
                            <Link
                                href={`/procurement/orders/${encodeURIComponent(purchaseOrderId)}?section=payable`}
                                target="_blank"
                                rel="noopener noreferrer"
                            />
                        }
                    >
                        查看采购付款
                        <ArrowUpRightIcon data-icon="inline-end" />
                        <span className="sr-only">（在新标签页打开）</span>
                    </Button>
                )}
            </div>
            <CollapsibleContent>
                <div className="flex flex-wrap items-center gap-x-5 gap-y-1 pt-2 pl-6 leading-6">
                    {operation.gate.requiredAmount != null && (
                        <span>
                            约定预付{" "}
                            <MoneyValue value={operation.gate.requiredAmount} />
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
        </Collapsible>
    )
}
