"use client"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { displayBusinessText } from "@/lib/display-name"
import type {
    PurchaseApiExecutionFunds,
    PurchaseOrderCenterView,
} from "../types"
import { PurchaseOrderDetailSection } from "./purchase-order-detail-section"

const FUNDS_STATUS_LABEL: Readonly<
    Record<PurchaseApiExecutionFunds["status"], string>
> = {
    not_reserved: "余额待占用",
    reserved: "资金已占用",
    awaiting_debit_verification: "扣款待核实",
    debit_pending_settlement: "扣款已核实 · 待核销",
    partially_settled: "部分核销",
    settled: "已核销",
    partially_returned: "部分返款已到账",
    returned: "返款已到账",
    released: "未使用占用已释放",
}

/** 发单资金保障与正式核销分开读取，不按供应商订单确认或取消推导实付和返款。 */
export function PurchaseOrderExecutionFunds({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    if (costMasked || !order.apiExecutionFunds?.length) return null
    return (
        <PurchaseOrderDetailSection width="form" title="预存款扣款与核销进度">
            <div className="divide-y divide-border/60 border-y border-border/60">
                {order.apiExecutionFunds.map((funds) => {
                    const execution = order.apiExecutions?.find(
                        (item) => item.orderId === funds.executionGroupId,
                    )
                    const label =
                        displayBusinessText(
                            execution?.orderNo,
                            funds.executionGroupId,
                        ) ?? "本采购资金保障"
                    return (
                        <article
                            key={funds.executionGroupId}
                            className="space-y-3 py-4"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <h3 className="font-medium">{label}</h3>
                                <BusinessStatusBadge
                                    context="detail"
                                    label={
                                        FUNDS_STATUS_LABEL[funds.status] ??
                                        "资金状态待核验"
                                    }
                                    tone={
                                        funds.status === "settled" ||
                                        funds.status === "returned"
                                            ? "success"
                                            : "info"
                                    }
                                />
                            </div>
                            {funds.reconciliationRequired ? (
                                <Alert variant="warning">
                                    <AlertDescription>
                                        账户仍有待核对事项，新增余额使用受限；原扣款、返款和既有订单继续调查。
                                    </AlertDescription>
                                </Alert>
                            ) : null}
                            {funds.status === "not_reserved" ? (
                                <p className="text-xs text-muted-foreground">
                                    暂未冻结预存款额度，请核对付款申请是否已批准、预存款余额及自动扣款授权是否充足。
                                </p>
                            ) : (
                                <dl className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3">
                                    {[
                                        ["批准使用额度", funds.approvedAmount],
                                        ["冻结中金额", funds.reservedAmount],
                                        [
                                            "供应商已扣款",
                                            funds.verifiedDebitAmount,
                                        ],
                                        ["已核销应付", funds.settledAmount],
                                        ["已退回预存款", funds.returnedAmount],
                                    ].map(([name, value]) => (
                                        <div key={name}>
                                            <dt className="text-xs text-muted-foreground">
                                                {name}
                                            </dt>
                                            <dd className="mt-1 text-body-compact font-medium">
                                                <MoneyValue
                                                    value={value}
                                                    display="exact"
                                                />
                                            </dd>
                                        </div>
                                    ))}
                                    {funds.pendingReturnAmount != null ? (
                                        <div>
                                            <dt className="text-xs text-muted-foreground">
                                                返款待核实
                                            </dt>
                                            <dd className="mt-1 text-body-compact font-medium">
                                                <MoneyValue
                                                    value={
                                                        funds.pendingReturnAmount
                                                    }
                                                    display="exact"
                                                />
                                            </dd>
                                        </div>
                                    ) : null}
                                </dl>
                            )}
                        </article>
                    )
                })}
            </div>
            <p className="text-xs text-muted-foreground">
                资金占用仅作为本次发单的保障，不计入已付。实际扣款核实后按原采购应付核销；取消不代表返款到账，供应商账户返款不记为银行现金退款。
            </p>
        </PurchaseOrderDetailSection>
    )
}
