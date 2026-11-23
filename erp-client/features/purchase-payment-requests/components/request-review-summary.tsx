"use client"

import {
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { compareDecimal } from "@/lib/fixed-decimal"
import {
    paymentReviewBasis,
    purchaseSummary,
    fulfillmentLabel,
} from "../lib/review"
import type { PaymentRequest } from "../types"

function fulfillmentStatusTone(status?: string | null): StatusTone {
    switch (status) {
        case "PARTIAL":
            return "info"
        case "COMPLETED":
        case "COMPLETE":
            return "success"
        case "NONE":
        case "PENDING":
        case "NOT_STARTED":
        default:
            return "neutral"
    }
}

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
    const progressByLabel = new Map<
        string,
        { label: string; tone: StatusTone }
    >()
    for (const line of request.lines) {
        const label = fulfillmentLabel[line.current_fulfillment ?? ""]
        if (label && !progressByLabel.has(label)) {
            progressByLabel.set(label, {
                label,
                tone: fulfillmentStatusTone(line.current_fulfillment),
            })
        }
    }
    const progress = [...progressByLabel.values()]
    const dates = [
        ...new Set(
            request.review_lines?.flatMap((line) => line.due_dates) ?? [],
        ),
    ].sort()
    const closed =
        compareDecimal(request.closed_amount, "0", 4) > 0 ||
        request.status === "closed"
    return (
        <DetailWorkspaceSection
            ariaLabel="付款申请与金额依据"
            className="py-5 first:pt-5 last:pb-5"
        >
            <div className="flex flex-wrap items-start justify-between gap-x-8 gap-y-3">
                <div className="min-w-0 flex-1 space-y-1">
                    <h2 className="text-section-title font-semibold wrap-anywhere tracking-tight text-foreground">
                        {purchaseSummary(request.lines)}
                    </h2>
                    <p className="num text-xs text-muted-foreground break-all">
                        {request.lines.length === 1
                            ? `关联采购单：${request.lines[0].purchase_order_no}`
                            : `合并 ${request.lines.length} 张采购单`}
                    </p>
                </div>
                <div className="min-w-0 space-y-1 @min-[640px]/payment-review:border-l @min-[640px]/payment-review:border-border/60 @min-[640px]/payment-review:pl-6">
                    <p className="text-xs text-muted-foreground">本次申请</p>
                    <p className="num text-value-summary font-semibold tracking-tight text-foreground">
                        <MoneyValue value={request.total_amount} />
                    </p>
                </div>
            </div>

            <div className="space-y-2 border-t border-border/60 pt-4">
                <DetailWorkspaceMetrics columns={4} variant="inline">
                    <DetailWorkspaceMetricItem
                        label="采购总额"
                        value={<MoneyValue value={basis.total} />}
                    />
                    <DetailWorkspaceMetricItem
                        label="有效已付"
                        value={<MoneyValue value={basis.paid} />}
                    />
                    <DetailWorkspaceMetricItem
                        label="其他申请占用"
                        value={<MoneyValue value={basis.reserved} />}
                    />
                    <DetailWorkspaceMetricItem
                        label="本次可申请"
                        value={<MoneyValue value={basis.available} />}
                        emphasis="primary"
                    />
                </DetailWorkspaceMetrics>
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

            <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border-t border-border/60 pt-3.5 text-body-compact">
                <p>
                    <span className="text-xs text-muted-foreground">
                        付款条件{" "}
                    </span>
                    {terms.length === 1 ? terms[0] : "见各采购单"}
                </p>
                <p>
                    <span className="text-xs text-muted-foreground">
                        希望付款日{" "}
                    </span>
                    <span className="num">{request.desired_payment_date}</span>
                </p>
                {dates.length > 0 && (
                    <p>
                        <span className="text-xs text-muted-foreground">
                            应付日期{" "}
                        </span>
                        <span className="num">
                            {dates.length === 1
                                ? dates[0]
                                : `${dates[0]} 至 ${dates[dates.length - 1]}`}
                        </span>
                    </p>
                )}
                {progress.length === 1 && (
                    <StatusBadge
                        label={progress[0].label}
                        tone={progress[0].tone}
                    />
                )}
            </div>

            {request.lines.some((line) => line.source_changed) && (
                <Alert variant="warning">
                    <AlertDescription>
                        采购或收款依据已变化，请核对后处理。
                    </AlertDescription>
                </Alert>
            )}
            {basis.exceeds && (
                <Alert variant="destructive">
                    <AlertDescription>
                        有采购单的申请未付金额超过可申请余额，请逐单核对。
                    </AlertDescription>
                </Alert>
            )}
            {(closed || compareDecimal(request.paid_amount, "0", 4) > 0) && (
                <div className="flex flex-wrap gap-x-6 gap-y-2 border-t border-border/60 pt-3 text-body-compact">
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
                        <p className="w-full text-xs text-muted-foreground">
                            本申请已终止后续付款：
                            {request.close_reason || "未记录原因"}
                            。冲正后不再恢复付款资格。
                        </p>
                    )}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}
