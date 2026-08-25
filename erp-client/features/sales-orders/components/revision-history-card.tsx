"use client"

import type { ReactNode } from "react"
import { HistoryIcon } from "lucide-react"

import { BusinessEmptyState, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Timeline,
    TimelineDescription,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import type { SalesOrderRevisionSnapshot } from "@/features/sales-orders/types"
import { cn } from "@/lib/utils"

type RevisionHistoryCardProps = {
    revisions: readonly SalesOrderRevisionSnapshot[]
    currentVersion: number | null
    contractRevisionLabel: string
}

function displayText(value?: string | null) {
    const trimmed = value?.trim() ?? ""
    return trimmed || "—"
}

function SnapshotField({
    label,
    value,
    numeric,
}: {
    label: string
    value: ReactNode
    numeric?: boolean
}) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd
                className={cn(
                    "mt-0.5 truncate text-sm font-medium text-foreground",
                    numeric && "num",
                )}
            >
                {value}
            </dd>
        </div>
    )
}

function invoiceLabel(rev: SalesOrderRevisionSnapshot) {
    const type = rev.invoiceType.trim()
    const tax = rev.taxPoint.trim()
    if (type && tax) return `${type} · ${tax}`
    return displayText(type || tax)
}

function contractLabel(
    rev: SalesOrderRevisionSnapshot,
    isCurrent: boolean,
    currentContractLabel: string,
) {
    if (isCurrent && currentContractLabel.trim()) return currentContractLabel
    return displayText(rev.contractRevisionLabel)
}

function VersionRecord({
    rev,
    isCurrent,
    currentContractLabel,
}: {
    rev: SalesOrderRevisionSnapshot
    isCurrent: boolean
    currentContractLabel: string
}) {
    const lines = rev.lines
    return (
        <article
            className={cn(
                "rounded-lg border px-3 py-3 sm:px-4",
                isCurrent
                    ? "border-primary/25 bg-primary/[0.03]"
                    : "border-border bg-card",
            )}
        >
            <div className="rounded-md bg-muted/40 px-3 py-2.5">
                <dl className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            当时成交金额
                        </dt>
                        <dd className="mt-0.5">
                            <MoneyValue
                                value={rev.amountGross || undefined}
                                taxBasis="gross"
                            />
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            不含税
                        </dt>
                        <dd className="mt-0.5">
                            <MoneyValue value={rev.amountNet || undefined} />
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">税额</dt>
                        <dd className="mt-0.5">
                            <MoneyValue value={rev.taxAmount || undefined} />
                        </dd>
                    </div>
                </dl>
            </div>

            <dl className="mt-3 grid grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2 xl:grid-cols-4">
                <SnapshotField
                    label="当时客户"
                    value={displayText(rev.customerSnapshot)}
                />
                <SnapshotField
                    label="当时合同"
                    value={contractLabel(rev, isCurrent, currentContractLabel)}
                    numeric
                />
                <SnapshotField
                    label="结算主体"
                    value={displayText(rev.settlementParty)}
                />
                <SnapshotField
                    label="付款条件"
                    value={displayText(rev.paymentTerm)}
                />
                <SnapshotField label="开票要求" value={invoiceLabel(rev)} />
                {rev.projectName.trim() ? (
                    <SnapshotField label="项目名称" value={rev.projectName} />
                ) : null}
                {rev.previousRevisionNo != null ? (
                    <SnapshotField
                        label="基于版本"
                        value={`v${rev.previousRevisionNo}`}
                        numeric
                    />
                ) : null}
            </dl>

            <div className="mt-3 border-t border-grid pt-3">
                <div className="mb-1.5 flex items-baseline justify-between gap-2">
                    <h3 className="text-xs font-medium text-muted-foreground">
                        明细摘要
                    </h3>
                    {lines.length > 0 ? (
                        <p className="num text-xs text-muted-foreground">
                            {lines.length} 项
                        </p>
                    ) : null}
                </div>
                {lines.length > 0 ? (
                    <ul className="divide-y divide-grid rounded-md border border-grid">
                        {lines.map((line) => (
                            <li
                                key={`${rev.revisionNo}-${line.lineNo}`}
                                className="flex items-start justify-between gap-3 px-3 py-1.5 text-sm"
                            >
                                <div className="min-w-0">
                                    <div className="truncate font-medium">
                                        {line.name}
                                    </div>
                                    {line.spec || line.unit ? (
                                        <div className="truncate text-xs text-muted-foreground">
                                            {[line.spec, line.unit]
                                                .filter(Boolean)
                                                .join(" · ")}
                                        </div>
                                    ) : null}
                                </div>
                                <MoneyValue
                                    value={line.amountGross || undefined}
                                    className="shrink-0"
                                />
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        {displayText(rev.lineSummary)}
                    </p>
                )}
            </div>

            {rev.businessRemark ? (
                <p className="mt-3 text-xs leading-5 text-muted-foreground">
                    {rev.businessRemark}
                </p>
            ) : null}
        </article>
    )
}

/**
 * 历史销售版本：当时的合同、客户、金额与明细快照，不被后来修改盖掉。
 */
export function RevisionHistoryCard({
    revisions,
    currentVersion,
    contractRevisionLabel,
}: RevisionHistoryCardProps) {
    const ordered = [...revisions].sort((a, b) => b.revisionNo - a.revisionNo)

    return (
        <div className="space-y-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                    <h2 className="text-sm font-medium">版本记录</h2>
                    <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
                        {`销售单生效后形成 v1，后续改单生效时保留旧版本。当时的合同、客户与金额不会被后来修改盖掉。${
                            contractRevisionLabel
                                ? ` 当前关联合同 ${contractRevisionLabel}。`
                                : ""
                        }`}
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-1.5">
                    {ordered.length > 0 ? (
                        <Badge variant="outline">{ordered.length} 个版本</Badge>
                    ) : null}
                    {currentVersion == null ? (
                        <Badge variant="outline">尚未生效</Badge>
                    ) : (
                        <Badge variant="secondary">
                            当前 v{currentVersion}
                        </Badge>
                    )}
                </div>
            </div>
            {ordered.length === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无正式版本"
                    description="销售单尚未生效，生效后会在这里留下 v1 及后续改单版本。"
                />
            ) : (
                <Timeline aria-label="销售版本时间线">
                    {ordered.map((rev) => {
                        const isCurrent = rev.revisionNo === currentVersion
                        return (
                            <TimelineItem key={rev.revisionNo}>
                                <TimelineMarker
                                    className={cn(
                                        isCurrent &&
                                            "border-primary/40 bg-primary/10 text-primary",
                                    )}
                                >
                                    <HistoryIcon aria-hidden="true" />
                                </TimelineMarker>
                                <TimelineHeader className="mb-2 items-center justify-between gap-2">
                                    <TimelineTitle className="flex flex-wrap items-center gap-1.5">
                                        <span className="num">
                                            v{rev.revisionNo}
                                        </span>
                                        {isCurrent ? (
                                            <Badge variant="info">
                                                当前在用
                                            </Badge>
                                        ) : (
                                            <Badge variant="outline">
                                                历史
                                            </Badge>
                                        )}
                                        {rev.note ? (
                                            <Badge variant="neutral">
                                                {rev.note}
                                            </Badge>
                                        ) : null}
                                        {rev.changeOrderId ? (
                                            <span className="num text-xs font-normal text-muted-foreground">
                                                改单 {rev.changeOrderId}
                                            </span>
                                        ) : null}
                                    </TimelineTitle>
                                    {rev.effectiveAt ? (
                                        <TimelineTime
                                            dateTime={rev.effectiveAt}
                                        >
                                            {rev.effectiveAt}
                                        </TimelineTime>
                                    ) : null}
                                </TimelineHeader>
                                <TimelineDescription className="text-foreground">
                                    <VersionRecord
                                        rev={rev}
                                        isCurrent={isCurrent}
                                        currentContractLabel={
                                            contractRevisionLabel
                                        }
                                    />
                                </TimelineDescription>
                            </TimelineItem>
                        )
                    })}
                </Timeline>
            )}
        </div>
    )
}
