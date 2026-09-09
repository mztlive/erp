"use client"

import Link from "next/link"

import { MoneyValue } from "@/components/business"
import {
    PreviewAmount,
    PreviewSection,
    PreviewFact,
    PreviewNote,
} from "@/components/business/financial-preview"
import { Skeleton } from "@/components/ui/skeleton"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { decimalProgressPercent } from "@/features/supplier-payables/lib/decimal-progress"
import type { PayableActivityItem } from "@/features/supplier-payables/lib/payable-preview-activity"
import type { PayableDetailView } from "@/features/supplier-payables/types"

export interface PayablePreviewBodyProps {
    payable: PayableDetailView["payable"]
    entries: PayableDetailView["entries"]
    activity: readonly PayableActivityItem[]
    paymentBlockedReason?: string
}

/** 应付详情抽屉加载占位。 */
export function PayablePreviewSkeleton() {
    return (
        <div className="flex flex-col gap-6 px-7 py-6">
            <Skeleton className="h-28 w-full" />
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-32 w-full" />
        </div>
    )
}

/**
 * 应付详情抽屉正文：双轨进度、构成分录、可点击往来记录。
 */
export function PayablePreviewBody({
    payable,
    entries,
    activity,
    paymentBlockedReason,
}: PayablePreviewBodyProps) {
    const paymentPercent = decimalProgressPercent(
        payable.settledTotal,
        payable.grossTotal,
    )
    const invoicePercent = decimalProgressPercent(
        payable.invoicedTotal,
        payable.grossTotal,
    )
    const gate = payable.paymentGateSummary
    const showGate = gate?.state === "BLOCKED"

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-auto px-7 py-6">
            <PreviewAmount label="待付金额（含税）" value={payable.openTotal}>
                <span>
                    应付总额 <MoneyValue value={payable.grossTotal} />
                </span>
                <span>
                    已付 <MoneyValue value={payable.settledTotal} />
                </span>
            </PreviewAmount>
            <PreviewSection title="结算进度">
                <ProgressTrackRow
                    label="付款进度"
                    percent={paymentPercent}
                    allocated={payable.settledTotal}
                    total={payable.grossTotal}
                    allocatedCaption="已付"
                    totalCaption="应付"
                />
                <ProgressTrackRow
                    label="收票进度"
                    percent={invoicePercent}
                    allocated={payable.invoicedTotal}
                    total={payable.grossTotal}
                    allocatedCaption="已收"
                    totalCaption="可收"
                />
                <dl className="space-y-3 pt-1">
                    <PreviewFact label="待收票（含税）">
                        <MoneyValue value={payable.openInvoiceableTotal} />
                    </PreviewFact>
                </dl>
                <PreviewNote>
                    付款与收票分别核销，已收票不代表已付款。
                </PreviewNote>
            </PreviewSection>
            <PreviewSection title="付款安排">
                <dl className="space-y-3">
                    <PreviewFact label="到期日">
                        <span className="num">{payable.dueDate}</span>
                        <span className="ml-2 text-xs text-muted-foreground">
                            {payable.dueStateLabel}
                        </span>
                    </PreviewFact>
                    <PreviewFact label="应付来源">
                        {payable.sourceTypeLabel}
                    </PreviewFact>
                    {payable.paymentRecipient ? (
                        <>
                            <PreviewFact label="收款户名">
                                {payable.paymentRecipient.accountName}
                            </PreviewFact>
                            <PreviewFact label="收款银行">
                                {payable.paymentRecipient.bankName}
                            </PreviewFact>
                            <PreviewFact label="收款账号">
                                <span className="num">
                                    {
                                        payable.paymentRecipient
                                            .accountNumberMasked
                                    }
                                </span>
                            </PreviewFact>
                        </>
                    ) : null}
                </dl>
                {showGate && gate ? (
                    <Alert variant="warning">
                        <AlertTitle>先款条件未满足</AlertTitle>
                        <AlertDescription>
                            已核销 <MoneyValue value={gate.allocated} /> / 要求{" "}
                            <MoneyValue value={gate.required} />
                            ，还差 <MoneyValue value={gate.gap} />。
                        </AlertDescription>
                    </Alert>
                ) : null}

                {paymentBlockedReason ? (
                    <PreviewNote>{paymentBlockedReason}</PreviewNote>
                ) : null}
            </PreviewSection>

            <PreviewSection title="应付构成">
                {entries.length === 0 ? (
                    <p className="text-sm text-muted-foreground">暂无分录</p>
                ) : (
                    <ul className="flex flex-col gap-2">
                        {entries.map((entry) => (
                            <li
                                key={entry.entryId}
                                className="flex items-start justify-between gap-5 border-b border-border py-3 text-sm last:border-b-0"
                            >
                                <div className="min-w-0">
                                    <p>
                                        {entry.entryTypeLabel}
                                        <span className="text-muted-foreground">
                                            {" "}
                                            ·{" "}
                                            {entry.direction === "increase"
                                                ? "增加"
                                                : "减少"}
                                        </span>
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        {entry.sourceLabel}
                                        {entry.dueDate
                                            ? ` · 到期 ${entry.dueDate}`
                                            : null}
                                    </p>
                                </div>
                                <MoneyValue
                                    value={entry.amount}
                                    className="shrink-0"
                                />
                            </li>
                        ))}
                    </ul>
                )}
            </PreviewSection>

            <PreviewSection title="核销记录">
                {activity.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚无付款或进项核销记录
                    </p>
                ) : (
                    <ul className="flex flex-col gap-1">
                        {activity.map((item) => (
                            <ActivityRow key={item.id} item={item} />
                        ))}
                    </ul>
                )}
            </PreviewSection>
        </div>
    )
}

function ProgressTrackRow({
    label,
    percent,
    allocated,
    total,
    allocatedCaption,
    totalCaption,
}: {
    label: string
    percent: number
    allocated: string
    total: string
    allocatedCaption: string
    totalCaption: string
}) {
    return (
        <Progress value={percent}>
            <ProgressLabel>{label}</ProgressLabel>
            <span className="ml-auto flex items-baseline gap-1 text-xs text-muted-foreground">
                <span>{allocatedCaption}</span>
                <MoneyValue value={allocated} className="text-xs" />
                <span>/ {totalCaption}</span>
                <MoneyValue value={total} className="text-xs" />
            </span>
        </Progress>
    )
}

function ActivityRow({ item }: { item: PayableActivityItem }) {
    const content = (
        <>
            <div className="min-w-0 flex-1">
                <p>
                    {item.trackLabel} · {item.actionLabel}
                </p>
                <p className="num mt-1 break-words text-xs text-muted-foreground">
                    {item.documentNo}
                </p>
                <p className="num mt-1 text-xs text-muted-foreground">
                    {formatDateTime(item.occurredAt, "full", "dash")}
                </p>
            </div>
            <MoneyValue value={item.amount} className="shrink-0" />
        </>
    )

    if (item.href) {
        return (
            <li>
                <Link
                    id={`supplier-payables-preview-activity-${toAutomationIdSegment(item.id)}-open`}
                    href={item.href}
                    className="flex items-center gap-2 rounded-lg px-2 py-2 text-sm hover:bg-muted"
                >
                    {content}
                </Link>
            </li>
        )
    }

    return (
        <li className="flex items-center gap-2 rounded-lg px-2 py-2 text-sm">
            {content}
        </li>
    )
}
