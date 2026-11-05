"use client"

import Link from "next/link"
import { ArrowUpRightIcon, CircleAlertIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
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
        <Alert id="prepayment-gate" variant="warning">
            <CircleAlertIcon aria-hidden="true" />
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-6 gap-y-3">
                <div className="min-w-0 flex-1 space-y-1.5">
                    <AlertTitle>{paymentReminder(operation)}</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-x-5 gap-y-1 text-xs">
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
                    </AlertDescription>
                </div>
                {canViewPayment && (
                    <Button
                        id="fulfillment-operations-gate-view-payment"
                        variant="outline"
                        size="sm"
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
        </Alert>
    )
}
