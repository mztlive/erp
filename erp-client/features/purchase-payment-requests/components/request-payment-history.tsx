"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { useRequestDownloadMutation } from "../hooks/queries"
import type { PaymentRequest } from "../types"

export function RequestPaymentHistory({
    request,
    workItemId,
}: {
    request: PaymentRequest
    workItemId?: string
}) {
    const download = useRequestDownloadMutation()
    return (
        <section
            className="space-y-4 border-t pt-5"
            aria-label="实际付款与回单"
        >
            <h3 className="font-medium">实际付款与回单</h3>
            {request.payments.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    尚未登记实际付款。
                </p>
            ) : (
                request.payments.map((payment) => (
                    <article
                        key={payment.payment_id}
                        className="space-y-3 rounded-lg border p-4 text-sm"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            <p className="num font-medium">
                                {payment.payment_no}
                            </p>
                            <p className="num text-xs text-muted-foreground">
                                {formatDateTime(
                                    new Date(
                                        payment.paid_at * 1000,
                                    ).toISOString(),
                                )}
                            </p>
                        </div>
                        <dl className="grid gap-3 sm:grid-cols-3">
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    原付款金额
                                </dt>
                                <dd className="mt-1">
                                    <MoneyValue value={payment.amount} />
                                </dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    有效付款金额
                                </dt>
                                <dd className="mt-1">
                                    <MoneyValue
                                        value={payment.effective_amount}
                                    />
                                </dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    已冲正金额
                                </dt>
                                <dd className="mt-1">
                                    <MoneyValue
                                        value={payment.reversed_amount}
                                    />
                                </dd>
                            </div>
                        </dl>
                        <ul className="space-y-1 text-xs text-muted-foreground">
                            {payment.allocations.map((allocation) => (
                                <li
                                    key={allocation.payable_account_id}
                                    className="flex flex-wrap justify-between gap-2"
                                >
                                    <span>
                                        原付款分配 ·{" "}
                                        {request.lines.find(
                                            (line) =>
                                                line.payable_account_id ===
                                                allocation.payable_account_id,
                                        )?.purchase_order_no ??
                                            "未记录采购单号"}
                                    </span>
                                    <MoneyValue value={allocation.amount} />
                                </li>
                            ))}
                        </ul>
                        {payment.receipt ? (
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <span className="min-w-0 break-all text-xs">
                                    {payment.receipt.file_name}
                                </span>
                                <Button
                                    id={`ppr-payment-${toAutomationIdSegment(payment.payment_id)}-receipt-download`}
                                    variant="outline"
                                    size="sm"
                                    disabled={download.isPending}
                                    onClick={() =>
                                        download.mutate({
                                            requestId: request.id,
                                            attachment: payment.receipt!,
                                            workItemId,
                                        })
                                    }
                                >
                                    下载付款回单
                                </Button>
                            </div>
                        ) : (
                            <p className="text-xs text-muted-foreground">
                                未记录付款回单。
                            </p>
                        )}
                    </article>
                ))
            )}
            {request.credit_uses.length > 0 && (
                <section className="space-y-3">
                    <h3 className="font-medium">留抵抵扣记录</h3>
                    {request.credit_uses.map((use) => (
                        <div
                            key={use.id}
                            className="flex flex-wrap justify-between gap-2 rounded-lg border p-4 text-sm"
                        >
                            <span>
                                {request.lines.find(
                                    (line) =>
                                        line.payable_account_id ===
                                        use.payable_account_id,
                                )?.purchase_order_no ?? "本申请采购"}{" "}
                                · 留抵抵扣
                            </span>
                            <MoneyValue value={use.amount} />
                        </div>
                    ))}
                </section>
            )}
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "付款回单读取失败")}
                </p>
            )}
        </section>
    )
}
