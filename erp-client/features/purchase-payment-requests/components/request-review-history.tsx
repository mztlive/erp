"use client"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { useRequestDownloadMutation } from "../hooks/queries"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
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
        <section
            aria-label="采购付款记录"
            className="space-y-3 border-t border-grid pt-4"
        >
            <h2 className="text-section-title font-semibold tracking-tight">
                付款记录
            </h2>
            {submissionVersion != null ? (
                <p className="text-xs text-muted-foreground">
                    当前为第 {submissionVersion}{" "}
                    次提交材料，不包含后续付款记录。
                </p>
            ) : reviews == null ? (
                <p className="text-xs text-muted-foreground">
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
                            className="space-y-2"
                        >
                            {reviews.length > 1 && (
                                <p className="num text-body-compact font-medium">
                                    {line?.purchase_order_no ?? "关联采购单"}
                                </p>
                            )}
                            {review.settlements.length === 0 ? (
                                <p className="text-xs text-muted-foreground">
                                    暂无付款核销记录
                                </p>
                            ) : (
                                <Table
                                    aria-label={`${line?.purchase_order_no ?? "采购单"}付款核销记录`}
                                    className="min-w-[680px] [--table-cell-block:0.5rem] [--table-cell-inline:0.75rem]"
                                >
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>
                                                付款 / 核销时间
                                            </TableHead>
                                            <TableHead>单号</TableHead>
                                            <TableHead>方式</TableHead>
                                            <TableHead>状态</TableHead>
                                            <TableHead data-align="end">
                                                有效金额
                                            </TableHead>
                                            <TableHead data-align="end">
                                                回单
                                            </TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
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
                                            const fullyReversed =
                                                compareDecimal(
                                                    row.effective_amount,
                                                    "0",
                                                    4,
                                                ) === 0
                                            return (
                                                <TableRow key={row.id}>
                                                    <TableCell className="num text-muted-foreground">
                                                        {formatDateTime(
                                                            new Date(
                                                                row.occurred_at *
                                                                    1000,
                                                            ).toISOString(),
                                                        )}
                                                    </TableCell>
                                                    <TableCell className="num whitespace-normal break-all">
                                                        {row.document_no || "—"}
                                                    </TableCell>
                                                    <TableCell>
                                                        {row.method_label}
                                                    </TableCell>
                                                    <TableCell>
                                                        <StatusBadge
                                                            tone={
                                                                fullyReversed
                                                                    ? "void"
                                                                    : reversed
                                                                      ? "warning"
                                                                      : "success"
                                                            }
                                                            label={
                                                                fullyReversed
                                                                    ? "已冲正"
                                                                    : reversed
                                                                      ? "部分冲正"
                                                                      : row.method_label ===
                                                                          "银行付款"
                                                                        ? "已付款"
                                                                        : "已核销"
                                                            }
                                                        />
                                                    </TableCell>
                                                    <TableCell data-align="end">
                                                        <p className="font-medium">
                                                            <MoneyValue
                                                                value={
                                                                    row.effective_amount
                                                                }
                                                            />
                                                        </p>
                                                        {reversed && (
                                                            <p className="mt-1 text-xs text-muted-foreground">
                                                                原核销{" "}
                                                                <MoneyValue
                                                                    value={
                                                                        row.amount
                                                                    }
                                                                />
                                                                {" · "}已冲正{" "}
                                                                <MoneyValue
                                                                    value={
                                                                        row.reversed_amount
                                                                    }
                                                                />
                                                            </p>
                                                        )}
                                                    </TableCell>
                                                    <TableCell data-align="end">
                                                        {receipt ? (
                                                            <Button
                                                                id={`ppr-review-${toAutomationIdSegment(review.payable_account_id)}-${toAutomationIdSegment(row.id)}-receipt`}
                                                                variant="outline"
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
                                                        ) : (
                                                            <span className="text-xs text-muted-foreground">
                                                                —
                                                            </span>
                                                        )}
                                                    </TableCell>
                                                </TableRow>
                                            )
                                        })}
                                    </TableBody>
                                </Table>
                            )}
                            {review.other_requests.length === 0 ? (
                                <p className="text-xs text-muted-foreground">
                                    无其他申请占用
                                </p>
                            ) : (
                                <div className="space-y-2">
                                    <h3 className="font-medium">
                                        其他申请{" "}
                                        <span className="text-xs font-normal text-muted-foreground">
                                            {review.other_requests.length} 笔
                                        </span>
                                    </h3>
                                    <Table
                                        aria-label={`${line?.purchase_order_no ?? "采购单"}其他付款申请`}
                                        className="min-w-[560px] [--table-cell-block:0.5rem] [--table-cell-inline:0.75rem]"
                                    >
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead>申请时间</TableHead>
                                                <TableHead>申请单号</TableHead>
                                                <TableHead>状态</TableHead>
                                                <TableHead data-align="end">
                                                    申请金额
                                                </TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {review.other_requests.map(
                                                (row) => (
                                                    <TableRow
                                                        key={row.request_id}
                                                    >
                                                        <TableCell className="num text-muted-foreground">
                                                            {formatDateTime(
                                                                new Date(
                                                                    row.created_at *
                                                                        1000,
                                                                ).toISOString(),
                                                            )}
                                                        </TableCell>
                                                        <TableCell className="num whitespace-normal break-all">
                                                            {row.request_no}
                                                        </TableCell>
                                                        <TableCell>
                                                            <StatusBadge
                                                                tone={
                                                                    requestStatusTone[
                                                                        row
                                                                            .status
                                                                    ]
                                                                }
                                                                label={
                                                                    requestStatusLabel[
                                                                        row
                                                                            .status
                                                                    ]
                                                                }
                                                            />
                                                        </TableCell>
                                                        <TableCell data-align="end">
                                                            <MoneyValue
                                                                value={
                                                                    row.amount
                                                                }
                                                            />
                                                        </TableCell>
                                                    </TableRow>
                                                ),
                                            )}
                                        </TableBody>
                                    </Table>
                                </div>
                            )}
                        </div>
                    )
                })
            )}
            {request.credit_uses.length > 0 && (
                <div className="space-y-3">
                    <h3 className="font-medium text-muted-foreground">
                        本申请留抵抵扣
                    </h3>
                    {request.credit_uses.map((row) => (
                        <div
                            key={row.id}
                            className="flex justify-between gap-4 text-body-compact"
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
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(download.error, "回单下载失败")}
                </p>
            )}
        </section>
    )
}
