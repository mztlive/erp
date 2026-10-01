"use client"

import { useState } from "react"
import { BusinessFailureState, BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useOfferingHistoryQuery } from "../hooks/queries"
import { OfferingTerms } from "./offering-terms"
import { offeringTime, offeringValidity } from "../lib/detail"

export function OfferingHistory({
    offeringId,
    canViewCosts,
}: {
    offeringId: string
    canViewCosts: boolean
}) {
    const query = useOfferingHistoryQuery(offeringId)
    const [selectedId, setSelectedId] = useState<string | null>(null)
    const revisions = query.data?.pages.flatMap((page) => page.items) ?? []
    const selected =
        revisions.find((row) => row.id === selectedId) ?? revisions[0]
    if (query.isPending)
        return (
            <p role="status" className="p-6 text-sm text-muted-foreground">
                正在加载条款历史…
            </p>
        )
    if (query.isError && !query.isFetchNextPageError)
        return (
            <BusinessFailureState
                title="条款历史加载失败"
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    if (!selected)
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂无条款历史"
                description="登记供给并保存商业条款后，这里展示真实的条款版本。"
            />
        )
    return (
        <div className="space-y-5">
            <p className="text-sm text-muted-foreground">
                按版本查看当时保存的价格与供应条件。可供状态和数量独立更新，不属于条款历史。
            </p>
            <div className="grid min-w-0 gap-6 lg:grid-cols-[240px_minmax(0,1fr)]">
                <nav aria-label="历史条款版本" className="space-y-2">
                    {revisions.map((revision) => (
                        <Button
                            key={revision.id}
                            id={`supplier-offering-history-version-${toAutomationIdSegment(revision.id)}`}
                            variant={
                                selected.id === revision.id
                                    ? "secondary"
                                    : "ghost"
                            }
                            aria-pressed={selected.id === revision.id}
                            className="h-auto w-full justify-start whitespace-normal px-3 py-3 text-left"
                            onClick={() => setSelectedId(revision.id)}
                        >
                            <span className="space-y-1">
                                <span className="flex items-center gap-2">
                                    条款 v{revision.revision_no}
                                    {revision.is_current ? (
                                        <Badge variant="outline">
                                            当前版本
                                        </Badge>
                                    ) : null}
                                </span>
                                <span className="num block text-xs font-normal text-muted-foreground">
                                    {offeringTime(revision.created_at)}
                                </span>
                            </span>
                        </Button>
                    ))}
                    {query.isFetchNextPageError ? (
                        <p role="alert" className="text-xs text-destructive">
                            更多版本加载失败，请重试。
                        </p>
                    ) : null}
                    {query.hasNextPage ? (
                        <Button
                            id="supplier-offering-history-load-more"
                            variant="outline"
                            className="w-full"
                            disabled={query.isFetchingNextPage}
                            onClick={() => void query.fetchNextPage()}
                        >
                            {query.isFetchingNextPage
                                ? "正在加载…"
                                : query.isFetchNextPageError
                                  ? "重试加载"
                                  : "加载更早版本"}
                        </Button>
                    ) : null}
                </nav>
                <section
                    className="min-w-0 space-y-5 lg:border-l lg:border-border lg:pl-6"
                    aria-label={`条款 v${selected.revision_no}`}
                >
                    <div>
                        <h2 className="text-base font-semibold">
                            条款 v{selected.revision_no}
                        </h2>
                        <p className="num mt-1 text-xs text-muted-foreground">
                            {offeringValidity(selected)}
                        </p>
                    </div>
                    <OfferingTerms
                        terms={selected}
                        canViewCosts={canViewCosts}
                    />
                </section>
            </div>
        </div>
    )
}
