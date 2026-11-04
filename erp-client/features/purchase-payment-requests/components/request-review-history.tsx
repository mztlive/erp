"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { useRequestDownloadMutation } from "../hooks/queries"
import { requestStatusLabel } from "../lib/display"
import type { PaymentRequest } from "../types"

export function RequestReviewHistory({
    request,
    workItemId,
    submissionVersion,
}: {
    request: PaymentRequest
    workItemId?: string
    submissionVersion?: number
}) {
    const download = useRequestDownloadMutation()
    const reviews = request.review_lines
    return (
        <section aria-label="采购付款记录" className="space-y-6 border-t pt-8">
            <h2 className="text-lg font-semibold">付款记录</h2>
            {submissionVersion != null ? (
                <p className="text-sm text-muted-foreground">
                    当前为第 {submissionVersion}{" "}
                    次提交材料，不包含后续付款记录。
                </p>
            ) : reviews == null ? (
                <p className="text-sm text-muted-foreground">
                    采购付款记录暂未取得。
                </p>
            ) : (
                reviews.map((review) => {
                    const line = request.lines.find(
                        (line) =>
                            line.payable_account_id ===
                            review.payable_account_id,
                    )
                    return (
                        <div
                            key={review.payable_account_id}
                            className="space-y-6"
                        >
                            {reviews.length > 1 && (
                                <p className="num text-sm font-medium">
                                    {line?.purchase_order_no ?? "关联采购单"}
                                </p>
                            )}
                            <div className="space-y-3">
                                <h3 className="text-sm font-medium text-muted-foreground">
                                    已付款 / 核销
                                </h3>
                                {review.settlements.length === 0 ? (
                                    <p className="py-2 text-sm text-muted-foreground">
                                        暂无付款核销记录
                                    </p>
                                ) : (
                                    <ul className="divide-y">
                                        {review.settlements.map((row) => {
                                            const receipt =
                                                request.payments.find(
                                                    (payment) =>
                                                        payment.payment_id ===
                                                        row.payment_id,
                                                )?.receipt
                                            const reversed =
                                                compareDecimal(
                                                    row.reversed_amount,
                                                    "0",
                                                    4,
                                                ) > 0
                                            return (
                                                <li
                                                    key={row.id}
                                                    className="flex flex-wrap items-center gap-x-6 gap-y-3 py-4 text-sm"
                                                >
                                                    <span className="num text-muted-foreground">
                                                        {formatDateTime(
                                                            new Date(
                                                                row.occurred_at *
                                                                    1000,
                                                            ).toISOString(),
                                                        )}
                                                    </span>
                                                    <div className="min-w-0 flex-1 space-y-1">
                                                        <p className="break-all">
                                                            {row.document_no ||
                                                                row.method_label}
                                                        </p>
                                                        {reversed && (
                                                            <p className="text-xs text-muted-foreground">
                                                                原核销{" "}
                                                                <MoneyValue
                                                                    value={
                                                                        row.amount
                                                                    }
                                                                />{" "}
                                                                · 已冲正{" "}
                                                                <MoneyValue
                                                                    value={
                                                                        row.reversed_amount
                                                                    }
                                                                />
                                                            </p>
                                                        )}
                                                    </div>
                                                    <Badge variant="secondary">
                                                        {compareDecimal(
                                                            row.effective_amount,
                                                            "0",
                                                            4,
                                                        ) === 0
                                                            ? "已冲正"
                                                            : reversed
                                                              ? "部分冲正"
                                                              : row.method_label ===
                                                                  "银行付款"
                                                                ? "已付款"
                                                                : row.method_label}
                                                    </Badge>
                                                    <span className="min-w-24 text-right">
                                                        <MoneyValue
                                                            value={
                                                                row.effective_amount
                                                            }
                                                        />
                                                    </span>
                                                    {receipt && (
                                                        <Button
                                                            id={`ppr-review-${toAutomationIdSegment(review.payable_account_id)}-${toAutomationIdSegment(row.id)}-receipt`}
                                                            variant="ghost"
                                                            size="sm"
                                                            disabled={
                                                                download.isPending
                                                            }
                                                            onClick={() =>
                                                                download.mutate(
                                                                    {
                                                                        requestId:
                                                                            request.id,
                                                                        attachment:
                                                                            receipt,
                                                                        workItemId,
                                                                    },
                                                                )
                                                            }
                                                        >
                                                            下载回单
                                                        </Button>
                                                    )}
                                                </li>
                                            )
                                        })}
                                    </ul>
                                )}
                            </div>
                            <div className="space-y-3">
                                <h3 className="text-sm font-medium text-muted-foreground">
                                    其他申请
                                </h3>
                                {review.other_requests.length === 0 ? (
                                    <p className="py-2 text-sm text-muted-foreground">
                                        无其他申请占用
                                    </p>
                                ) : (
                                    <ul className="divide-y">
                                        {review.other_requests.map((row) => (
                                            <li
                                                key={row.request_id}
                                                className="flex flex-wrap items-center gap-x-6 gap-y-3 py-4 text-sm"
                                            >
                                                <span className="num text-muted-foreground">
                                                    {formatDateTime(
                                                        new Date(
                                                            row.created_at *
                                                                1000,
                                                        ).toISOString(),
                                                    )}
                                                </span>
                                                <span className="min-w-0 flex-1 break-all">
                                                    {row.request_no}
                                                </span>
                                                <Badge variant="secondary">
                                                    {
                                                        requestStatusLabel[
                                                            row.status
                                                        ]
                                                    }
                                                </Badge>
                                                <span className="min-w-24 text-right">
                                                    <MoneyValue
                                                        value={row.amount}
                                                    />
                                                </span>
                                            </li>
                                        ))}
                                    </ul>
                                )}
                            </div>
                        </div>
                    )
                })
            )}
            {request.credit_uses.length > 0 && (
                <div className="space-y-3">
                    <h3 className="text-sm font-medium text-muted-foreground">
                        本申请留抵抵扣
                    </h3>
                    {request.credit_uses.map((row) => (
                        <div
                            key={row.id}
                            className="flex justify-between gap-4 text-sm"
                        >
                            <span>
                                {request.lines.find(
                                    (line) =>
                                        line.payable_account_id ===
                                        row.payable_account_id,
                                )?.purchase_order_no ?? "关联采购单"}
                            </span>
                            <MoneyValue value={row.amount} />
                        </div>
                    ))}
                </div>
            )}
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "回单下载失败")}
                </p>
            )}
        </section>
    )
}
