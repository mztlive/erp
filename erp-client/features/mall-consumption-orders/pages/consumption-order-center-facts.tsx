"use client"

import { BusinessStatusBadge, DocumentSection } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { MallOrderFactView } from "@/features/mall-consumption-orders/types"
import {
    FACT_TYPE_LABEL,
    FACT_TYPE_TONE,
    PROCESSING_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/datetime"

function FactCard({
    fact,
    selected,
    onSelect,
}: {
    fact: MallOrderFactView
    selected: boolean
    onSelect: () => void
}) {
    return (
        <button
            type="button"
            onClick={onSelect}
            className={cn(
                "w-full rounded-lg p-3 text-left transition-colors ring-1",
                selected
                    ? "bg-primary/5 ring-primary/40"
                    : "bg-card ring-foreground/[0.04] hover:bg-muted/40",
            )}
            aria-current={selected ? "true" : undefined}
        >
            <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                    context="detail"
                    label={FACT_TYPE_LABEL[fact.factType]}
                    tone={FACT_TYPE_TONE[fact.factType]}
                />
                <Badge variant="outline">
                    {fact.dataSource === "BACKFILL" ? "回填" : "实时"}
                </Badge>
                <span className="num text-xs text-muted-foreground">
                    {fact.businessFactKeySummary}
                </span>
            </div>
            <dl className="mt-2 grid gap-1 text-xs sm:grid-cols-2">
                <div>
                    <dt className="text-muted-foreground">发生时间</dt>
                    <dd className="num">
                        {formatDateTime(fact.occurredAt, "default")}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">接收时间</dt>
                    <dd className="num">
                        {formatDateTime(fact.receivedAt, "default")}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">商城版本</dt>
                    <dd className="num">{fact.externalOrderVersion}</dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">处理状态</dt>
                    <dd>{PROCESSING_STATUS_LABEL[fact.processingStatus]}</dd>
                </div>
                {fact.afterSalesRequestId ? (
                    <div>
                        <dt className="text-muted-foreground">售后请求</dt>
                        <dd className="num">{fact.afterSalesRequestId}</dd>
                    </div>
                ) : null}
            </dl>
            {Object.keys(fact.resultDetails).length > 0 ? (
                <ul className="mt-2 space-y-0.5 text-xs text-muted-foreground">
                    {Object.entries(fact.resultDetails).map(([k, v]) => (
                        <li key={k}>
                            {k}:{" "}
                            <span className="text-foreground">
                                {String(v ?? "—")}
                            </span>
                        </li>
                    ))}
                </ul>
            ) : null}
        </button>
    )
}

export function FactsSection({
    facts,
    selectedFactId,
    onSelectFact,
}: {
    facts: MallOrderFactView[]
    selectedFactId: string | undefined
    onSelectFact: (factId: string) => void
}) {
    return (
        <DocumentSection
            title="五类关键记录时间线"
            description="以业务发生时间排序，并展示接收时间。多次部分退款与余额恢复逐笔展示，不按订单号合并。"
        >
            <div className="grid gap-3 lg:grid-cols-2">
                {facts.map((fact) => (
                    <FactCard
                        key={fact.factId}
                        fact={fact}
                        selected={fact.factId === selectedFactId}
                        onSelect={() => onSelectFact(fact.factId)}
                    />
                ))}
            </div>
        </DocumentSection>
    )
}
