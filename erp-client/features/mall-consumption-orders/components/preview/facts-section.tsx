"use client"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { MallOrderFactView } from "@/features/mall-consumption-orders/types"
import { FACT_TYPE_LABEL, FACT_TYPE_TONE } from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { SectionTitle } from "./section-title"

type Props = {
    facts: MallOrderFactView[]
}

export function FactsSection({ facts }: Props) {
    const sortedFacts = [...facts].sort(
        (a, b) =>
            new Date(a.occurredAt).getTime() - new Date(b.occurredAt).getTime(),
    )

    return (
        <section className="space-y-2" aria-label="关键记录">
            <SectionTitle>关键记录</SectionTitle>
            {sortedFacts.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无关键记录</p>
            ) : (
                <ul className="space-y-2">
                    {sortedFacts.map((fact) => (
                        <FactRow key={fact.factId} fact={fact} />
                    ))}
                </ul>
            )}
        </section>
    )
}

function FactRow({ fact }: { fact: MallOrderFactView }) {
    return (
        <li className="rounded-lg border border-border bg-card px-3 py-2 text-xs">
            <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                    context="list"
                    label={FACT_TYPE_LABEL[fact.factType]}
                    tone={FACT_TYPE_TONE[fact.factType]}
                />
                <Badge variant="outline">
                    {fact.dataSource === "BACKFILL" ? "回填" : "实时"}
                </Badge>
                <span className="num text-muted-foreground">
                    {fact.businessFactKeySummary}
                </span>
            </div>
            <div className="mt-1 text-muted-foreground">
                发生{" "}
                <span className="num">
                    {formatDateTime(fact.occurredAt, "default")}
                </span>{" "}
                · 接收{" "}
                <span className="num">
                    {formatDateTime(fact.receivedAt, "default")}
                </span>
            </div>
        </li>
    )
}
