"use client"

import { HistoryIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import type { SalesOrderRevisionSnapshot } from "@/features/sales-orders/types"

type RevisionHistoryCardProps = {
  revisions: readonly SalesOrderRevisionSnapshot[]
  currentVersion: number
  contractRevisionLabel: string
}

/**
 * 历史销售版本快照：合同/主数据精确修订，不被当前值覆盖。
 */
export function RevisionHistoryCard({
  revisions,
  currentVersion,
  contractRevisionLabel,
}: RevisionHistoryCardProps) {
  const ordered = [...revisions].sort((a, b) => b.revisionNo - a.revisionNo)

  return (
    <Card size="sm">
      <CardHeader className="border-b">
        <div className="flex flex-wrap items-center gap-2">
          <HistoryIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <CardTitle>版本与审计记录</CardTitle>
          <Badge variant="secondary">当前 v{currentVersion}</Badge>
        </div>
        <CardDescription>
          当前合同修订 {contractRevisionLabel}
          。历史版本保留精确合同/主数据修订与金额记录，不会被当前主数据回填覆盖。
        </CardDescription>
      </CardHeader>
      <CardContent>
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
                    <span className="num font-medium">v{rev.revisionNo}</span>
                    {isCurrent ? (
                      <Badge variant="info">当前正式</Badge>
                    ) : (
                      <Badge variant="outline">历史记录</Badge>
                    )}
                    {rev.changeOrderId ? (
                      <span className="num text-xs text-muted-foreground">
                        {rev.changeOrderId}
                      </span>
                    ) : null}
                  </div>
                  <span className="num text-xs text-muted-foreground">
                    {rev.effectiveAt}
                  </span>
                </div>
                <dl className="mt-2 grid gap-1 text-xs sm:grid-cols-2">
                  <div>
                    <dt className="text-muted-foreground">合同修订</dt>
                    <dd className="num font-medium">
                      {rev.contractRevisionLabel}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-muted-foreground">客户记录</dt>
                    <dd className="font-medium">{rev.customerSnapshot}</dd>
                  </div>
                  <div>
                    <dt className="text-muted-foreground">含税金额记录</dt>
                    <dd>
                      <MoneyValue value={rev.amountGross} taxBasis="gross" />
                    </dd>
                  </div>
                  <div className="sm:col-span-2">
                    <dt className="text-muted-foreground">明细摘要</dt>
                    <dd className="font-medium">{rev.lineSummary}</dd>
                  </div>
                </dl>
                <p className="mt-2 text-xs text-muted-foreground">{rev.note}</p>
              </li>
            )
          })}
        </ol>
      </CardContent>
    </Card>
  )
}
