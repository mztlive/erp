"use client"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderRevisionSnapshot } from "@/features/sales-orders/types"

type RevisionHistoryCardProps = {
    revisions: readonly SalesOrderRevisionSnapshot[]
    currentVersion: number
    contractRevisionLabel: string
}

/**
 * 历史销售版本：当时的合同、客户与金额快照，不被后来修改盖掉。
 */
export function RevisionHistoryCard({
    revisions,
    currentVersion,
    contractRevisionLabel,
}: RevisionHistoryCardProps) {
    const ordered = [...revisions].sort((a, b) => b.revisionNo - a.revisionNo)

    return (
        <div className="space-y-3">
            <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0">
                    <h2 className="text-sm font-medium">历史版本</h2>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        关联合同 {contractRevisionLabel || "—"}
                        。改单生效后旧版本仍保留，方便对照当时卖了什么、多少钱。
                    </p>
                </div>
                <Badge variant="secondary">当前 v{currentVersion}</Badge>
            </div>
            {ordered.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无历史版本</p>
            ) : (
                <ol className="space-y-3" aria-label="销售版本时间线">
                    {ordered.map((rev) => {
                        const isCurrent = rev.revisionNo === currentVersion
                        return (
                            <li
                                key={rev.revisionNo}
                                className="rounded-lg border border-border px-3 py-2.5"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="flex items-center gap-2">
                                        <span className="num text-sm font-medium">
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
                                        {rev.changeOrderId ? (
                                            <span className="num text-xs text-muted-foreground">
                                                改单 {rev.changeOrderId}
                                            </span>
                                        ) : null}
                                    </div>
                                    <span className="num text-xs text-muted-foreground">
                                        {rev.effectiveAt}
                                    </span>
                                </div>
                                <dl className="mt-2 grid gap-1 text-xs sm:grid-cols-2">
                                    <div>
                                        <dt className="text-muted-foreground">
                                            当时合同
                                        </dt>
                                        <dd className="num font-medium">
                                            {rev.contractRevisionLabel || "—"}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-muted-foreground">
                                            当时客户
                                        </dt>
                                        <dd className="font-medium">
                                            {rev.customerSnapshot || "—"}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-muted-foreground">
                                            当时成交金额
                                        </dt>
                                        <dd>
                                            <MoneyValue
                                                value={rev.amountGross}
                                                taxBasis="gross"
                                            />
                                        </dd>
                                    </div>
                                    <div className="sm:col-span-2">
                                        <dt className="text-muted-foreground">
                                            明细摘要
                                        </dt>
                                        <dd className="font-medium">
                                            {rev.lineSummary || "—"}
                                        </dd>
                                    </div>
                                </dl>
                                {rev.note ? (
                                    <p className="mt-2 text-xs text-muted-foreground">
                                        {rev.note}
                                    </p>
                                ) : null}
                            </li>
                        )
                    })}
                </ol>
            )}
        </div>
    )
}
