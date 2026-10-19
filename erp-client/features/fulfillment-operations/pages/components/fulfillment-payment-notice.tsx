"use client"

import Link from "next/link"
import { ArrowUpRightIcon, CircleAlertIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { gateBlockedMessage } from "../../lib/validation"
import { OPERATION_ACTION_LABEL, type FulfillmentOperation } from "../../types"

/** 先展示实际付款缺口；跳转只读采购票款资料，不直接新建付款申请。 */
export function FulfillmentPaymentNotice({
    operation,
    canSaveDraft,
}: {
    operation: FulfillmentOperation
    canSaveDraft: boolean
}) {
    const profile = useAccountProfileQuery()
    const purchaseOrderId = operation.source.purchaseOrderId
    const canViewPayment =
        purchaseOrderId &&
        hasPermission(profile.data?.permissions, "purchase_order:detail")

    return (
        <Alert id="prepayment-gate" variant="warning">
            <CircleAlertIcon aria-hidden="true" />
            <AlertTitle>
                待满足付款条件 · 暂不可
                {OPERATION_ACTION_LABEL[operation.operationType]}
            </AlertTitle>
            <AlertDescription className="space-y-3">
                <p className="font-medium">{gateBlockedMessage(operation)}</p>
                <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-xs">
                    {operation.gate.requiredAmount != null ? (
                        <span>
                            履约前需付{" "}
                            <MoneyValue value={operation.gate.requiredAmount} />
                        </span>
                    ) : null}
                    {operation.gate.effectivePaidAmount != null ? (
                        <span>
                            有效已付{" "}
                            <MoneyValue
                                value={operation.gate.effectivePaidAmount}
                            />
                        </span>
                    ) : null}
                </div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <p className="text-xs">
                        付款申请或审批通过不代表已支付。
                        {canSaveDraft
                            ? "可以先填写并保存草稿，付款条件满足后再确认。"
                            : null}
                    </p>
                    {canViewPayment ? (
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
                    ) : null}
                </div>
            </AlertDescription>
        </Alert>
    )
}
