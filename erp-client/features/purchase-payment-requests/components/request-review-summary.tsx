"use client"

import {
    MoneyValue,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
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
            className={cn(
                workspaceTaskSurfacePadClassName,
                "space-y-4 border-b border-grid bg-info-soft/30 py-4",
            )}
        >
            <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
                <div className="min-w-0 flex-1 space-y-1">
                    <h2 className="text-base font-semibold leading-snug wrap-anywhere">
                        {purchaseSummary(request.lines)}
                    </h2>
                    <p className="text-sm wrap-anywhere">
                        {request.supplier_name}
                    </p>
                    <p className="num text-xs text-muted-foreground break-all">
                        {request.lines.length === 1
                            ? request.lines[0].purchase_order_no
                            : `共 ${request.lines.length} 张采购单`}
                    </p>
                </div>
                <div className="min-w-0 space-y-1 @min-[640px]/payment-review:border-l @min-[640px]/payment-review:border-grid @min-[640px]/payment-review:pl-6">
                    <p className="text-sm text-muted-foreground">本次申请</p>
                    <p className="text-2xl font-semibold tracking-tight">
                        <MoneyValue value={request.total_amount} />
                    </p>
                </div>
            </div>
            <div className="space-y-2">
                <dl className="grid grid-cols-2 gap-x-6 gap-y-3 @min-[640px]/payment-review:grid-cols-4">
                    {[
                        ["采购总额", basis.total],
                        ["有效已付", basis.paid],
                        ["其他申请占用", basis.reserved],
                        ["本次可申请", basis.available],
                    ].map(([label, value]) => (
                        <div key={label} className="min-w-0 space-y-1">
                            <dt className="text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd
                                className={
                                    label === "本次可申请"
                                        ? "text-base font-semibold"
                                        : "text-base"
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
            <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border-t border-grid pt-3 text-sm">
                <p>
                    <span className="text-muted-foreground">付款条件 </span>
                    {terms.length === 1 ? terms[0] : "见各采购单"}
                </p>
                <p>
                    <span className="text-muted-foreground">希望付款日 </span>
                    <span className="num">{request.desired_payment_date}</span>
                </p>
                {dates.length > 0 && (
                    <p>
                        <span className="text-muted-foreground">应付日期 </span>
                        <span className="num">
                            {dates.length === 1
                                ? dates[0]
                                : `${dates[0]} 至 ${dates[dates.length - 1]}`}
                        </span>
                    </p>
                )}
                {progress.length === 1 && (
                    <Badge variant="secondary">{progress[0]}</Badge>
                )}
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
                <div className="flex flex-wrap gap-x-6 gap-y-2 border-t border-grid pt-3 text-sm">
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
                            本申请已终止后续付款：
                            {request.close_reason || "未记录原因"}
                            。冲正后不再恢复付款资格。
                        </p>
                    )}
                </div>
            )}
        </section>
    )
}
