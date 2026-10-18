"use client"

import { MoneyValue } from "@/components/business/values"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ReturnSettlementDecision } from "@/features/purchase-orders/direct-return-types"

/** 将当前可办理金额与采购方案分开，说明退款、留抵和红票的办理条件。 */
export function ReturnFinanceSummary({
    invoiceTask,
    refund,
    credit,
    redInvoice,
    decision,
}: {
    invoiceTask: boolean
    refund: string
    credit: string
    redInvoice: string
    decision: ReturnSettlementDecision
}) {
    const amounts = invoiceTask
        ? [["本采购尚需冲减的进项票额", redInvoice]]
        : [
              ["本次可提交现金退款", refund],
              ["本次可提交留抵", credit],
          ]
    return (
        <section
            className="space-y-4 @container/return-finance"
            aria-label="当前财务处理事项"
        >
            <Alert variant="info">
                <AlertTitle>
                    {invoiceTask
                        ? "当前需要你处理：登记退货进项红票"
                        : "当前需要你处理：按采购确认方案办理退货款"}
                </AlertTitle>
                <AlertDescription>
                    {invoiceTask
                        ? "核对原蓝票，按实际收到的红票登记本采购的冲减金额。票额差额处理完成后，本条红票待办结束。"
                        : "现金退款须实际到账后登记；留抵须提交审批，通过后形成供应商余额。提交审批不代表已入账，退货款差额处理完成后本条待办才结束。"}
                </AlertDescription>
            </Alert>
            <dl className="grid gap-3 @sm/return-finance:grid-cols-2">
                {amounts.map(([label, amount]) => (
                    <div
                        key={label}
                        className="space-y-2 rounded-lg border bg-muted/30 p-4"
                    >
                        <dt className="text-sm text-muted-foreground">
                            {label}
                        </dt>
                        <dd>
                            <MoneyValue
                                value={amount}
                                className="text-xl font-semibold"
                            />
                        </dd>
                    </div>
                ))}
            </dl>
            {!invoiceTask ? (
                <p className="text-sm text-muted-foreground">
                    以上为当前方案扣除已提交金额后的剩余额度；审批中的金额不重复提交。
                </p>
            ) : null}
            <section
                className="space-y-2 rounded-lg border p-4 text-sm"
                aria-label="采购确认方案"
            >
                <h3 className="font-medium">采购已确认的方案与供应商依据</h3>
                {!invoiceTask ? (
                    <p className="flex flex-wrap gap-x-5 gap-y-2">
                        <span>
                            现金退款总额{" "}
                            <MoneyValue value={decision.refund_amount} />
                        </span>
                        <span>
                            留抵总额{" "}
                            <MoneyValue value={decision.credit_amount} />
                        </span>
                    </p>
                ) : null}
                <p className="whitespace-pre-wrap text-muted-foreground">
                    {decision.supplier_confirmation}
                </p>
            </section>
        </section>
    )
}
