"use client"

import Link from "next/link"

import { MoneyValue } from "@/components/business"
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
        <div className="flex flex-col gap-4 p-6">
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
        <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-auto p-6">
            {showGate && gate ? (
                <Alert variant="warning">
                    <AlertTitle>先款条件未满足</AlertTitle>
                    <AlertDescription>
                        {gate.message} · 已核销 {gate.allocated} / 门槛{" "}
                        {gate.required} · 差额 {gate.gap}
                    </AlertDescription>
                </Alert>
            ) : null}

            <section className="flex flex-col gap-4 rounded-lg border border-border bg-card p-4">
                <h3 className="text-sm font-semibold">进度</h3>
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
            </section>

            <section className="flex flex-col gap-3">
                <h3 className="text-sm font-semibold">构成</h3>
                {entries.length === 0 ? (
                    <p className="text-sm text-muted-foreground">暂无分录</p>
                ) : (
                    <ul className="flex flex-col gap-2">
                        {entries.map((entry) => (
                            <li
                                key={entry.entryId}
                                className="flex items-start justify-between gap-3 rounded-lg border border-border px-3 py-2 text-sm"
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
            </section>

            <section className="flex flex-col gap-3">
                <h3 className="text-sm font-semibold">往来</h3>
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
            </section>

            {paymentBlockedReason ? (
                <p className="text-xs text-muted-foreground">
                    {paymentBlockedReason}
                </p>
            ) : null}
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
            <span className="w-24 shrink-0 text-xs text-muted-foreground">
                {formatDateTime(item.occurredAt, "monthDay", "dash")}
            </span>
            <span className="min-w-0 flex-1 truncate">
                {item.trackLabel} · {item.actionLabel}
                <span className="text-muted-foreground">
                    {" "}
                    · {item.documentNo}
                </span>
            </span>
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
