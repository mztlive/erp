"use client"

import { DataFreshness } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import type { CardFundsReviewQueueView } from "@/features/card-funds-review/types"
import { freshnessText } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"

/** 页头：标题 + 数据更新时间 + 队列位置播报。 */
export function ReviewPageHeader({
    context,
}: {
    context: CardFundsReviewQueueView["context"] | undefined
}) {
    return (
        <ListWorkspaceHeader eyebrow="财务" title="卡券票款复核">
            <div className="flex flex-wrap items-center gap-3">
                <DataFreshness
                    updatedAt={
                        context?.queueContextUpdatedAt
                            ? formatDateTime(
                                  context.queueContextUpdatedAt,
                                  "full",
                              )
                            : "刚刚"
                    }
                    dateTime={context?.queueContextUpdatedAt}
                    state="fresh"
                    label={freshnessText.queueUpdatedAt}
                />
                <span
                    className="text-xs text-muted-foreground"
                    aria-live="polite"
                >
                    {context?.filterSummary ?? "仅我的"} · 第{" "}
                    {context?.position ?? 0}/{context?.total ?? 0} 项
                </span>
            </div>
        </ListWorkspaceHeader>
    )
}
