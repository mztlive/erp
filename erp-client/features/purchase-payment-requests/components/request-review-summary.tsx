"use client"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    paymentReviewBasis,
    purchaseSummary,
    fulfillmentLabel,
} from "../lib/review"
import type { PaymentRequest } from "../types"

export function RequestReviewSummary({
    request,
    submissionVersion,
}: {
    request: PaymentRequest
    submissionVersion?: number
}) {
    const basis = paymentReviewBasis(request, submissionVersion != null)
    const terms = [
        ...new Set(request.lines.map((line) => line.payment_term_name)),
    ]
    const progress = [
        ...new Set(
            request.lines
                .map((line) => fulfillmentLabel[line.current_fulfillment ?? ""])
                .filter(Boolean),
        ),
    ]
    const dates = [
        ...new Set(
            request.review_lines?.flatMap((line) => line.due_dates) ?? [],
        ),
    ].sort()
    const closed =
        compareDecimal(request.closed_amount, "0", 4) > 0 ||
        request.status === "closed"
    return (
        <section
            aria-label="付款申请与金额依据"
            className="space-y-8 rounded-xl border border-border/60 bg-card p-6 sm:p-8"
        >
            <div className="flex flex-wrap items-start justify-between gap-6">
                <div className="min-w-0 flex-1 space-y-3">
                    <h2 className="text-xl font-semibold leading-snug sm:text-2xl">
                        {purchaseSummary(request.lines)}
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        {request.lines.length === 1
                            ? request.lines[0].purchase_order_no
                            : `共 ${request.lines.length} 张采购单`}
                    </p>
                </div>
                <div className="space-y-2 sm:border-l sm:pl-8">
                    <p className="text-sm text-muted-foreground">本次申请</p>
                    <p className="text-3xl font-semibold tracking-tight sm:text-4xl">
                        <MoneyValue value={request.total_amount} />
                    </p>
                </div>
            </div>
            <div className="space-y-4">
                <dl className="grid grid-cols-2 gap-x-6 gap-y-5 sm:grid-cols-4">
                    {[
                        ["采购总额", basis.total],
                        ["有效已付", basis.paid],
                        ["其他申请占用", basis.reserved],
                        ["本次可申请", basis.available],
                    ].map(([label, value]) => (
                        <div key={label} className="space-y-2">
                            <dt className="text-sm text-muted-foreground">
                                {label}
                            </dt>
                            <dd
                                className={
                                    label === "本次可申请"
                                        ? "text-xl font-semibold"
                                        : "text-lg"
                                }
                            >
                                <MoneyValue value={value} />
                            </dd>
                        </div>
                    ))}
                </dl>
                <p className="text-xs text-muted-foreground">
                    {basis.current
                        ? "当前采购余额 · 可申请金额包含本申请未付占用"
                        : submissionVersion
                          ? `第 ${submissionVersion} 次提交时的金额依据`
                          : request.status === "draft"
                            ? "保存时的金额依据"
                            : "提交时的金额依据"}
                </p>
            </div>
            <div className="space-y-4 border-t pt-5 text-sm">
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <p>
                        {terms.length === 1 ? terms[0] : "付款条件见各采购单"}
                    </p>
                    {dates.length > 0 && (
                        <p className="num text-muted-foreground">
                            应付日期{" "}
                            {dates.length === 1
                                ? dates[0]
                                : `${dates[0]} 至 ${dates[dates.length - 1]}`}
                        </p>
                    )}
                </div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <p>
                        <span className="text-muted-foreground">
                            希望付款日{" "}
                        </span>
                        <span className="num">
                            {request.desired_payment_date}
                        </span>
                    </p>
                    {progress.length === 1 && (
                        <Badge variant="secondary">{progress[0]}</Badge>
                    )}
                </div>
            </div>
            {request.lines.some((line) => line.source_changed) && (
                <p
                    role="alert"
                    className="rounded-md bg-warning-soft px-4 py-3 text-sm text-warning-soft-foreground"
                >
                    采购或收款依据已变化，请核对后处理。
                </p>
            )}
            {basis.exceeds && (
                <p role="alert" className="text-sm text-destructive">
                    有采购单的申请未付金额超过可申请余额，请逐单核对。
                </p>
            )}
            {(closed || compareDecimal(request.paid_amount, "0", 4) > 0) && (
                <div className="flex flex-wrap gap-x-6 gap-y-2 border-t pt-4 text-sm">
                    <span>
                        本申请已付 <MoneyValue value={request.paid_amount} />
                    </span>
                    <span>
                        本申请待付 <MoneyValue value={request.unpaid_amount} />
                    </span>
                    {closed && (
                        <span>
                            已关闭未付{" "}
                            <MoneyValue value={request.closed_amount} />
                        </span>
                    )}
                    {request.status === "closed" && (
                        <p className="w-full text-muted-foreground">
                            付款授权已终止：
                            {request.close_reason || "未记录原因"}
                            。冲正后不恢复付款授权。
                        </p>
                    )}
                </div>
            )}
        </section>
    )
}
