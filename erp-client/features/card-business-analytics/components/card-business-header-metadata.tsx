import { DataFreshness } from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import type { CardBusinessAnalyticsView } from "../types"
import { mapFreshnessUi } from "../lib/freshness"

export interface CardBusinessHeaderMetadataProps {
    freshness: CardBusinessAnalyticsView["freshness"]
    refreshing: boolean
    refreshFailed: string | null
}

/** 页头元数据：分析汇总新鲜度与同步/业务记录/余额快照水位时间。 */
export function CardBusinessHeaderMetadata({
    freshness,
    refreshing,
    refreshFailed,
}: CardBusinessHeaderMetadataProps) {
    const freshnessUi = mapFreshnessUi(freshness.state, {
        refreshFailed: Boolean(refreshFailed),
        refreshing,
        breached: freshness.slaState === "BREACHED",
    })

    return (
        <div className="flex flex-col gap-1">
            <DataFreshness
                updatedAt={formatDateTime(
                    freshness.projectionUpdatedAt,
                    "full",
                )}
                dateTime={freshness.projectionUpdatedAt}
                state={freshnessUi.uiState}
                statusLabel={freshnessUi.statusLabel}
                label="分析汇总"
            />
            <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
                <span>
                    同步{" "}
                    <time
                        className="num"
                        dateTime={freshness.consumedOutboxWatermark}
                    >
                        {formatDateTime(
                            freshness.consumedOutboxWatermark,
                            "full",
                        )}
                    </time>
                </span>
                <span aria-hidden>·</span>
                <span>
                    业务记录{" "}
                    <time
                        className="num"
                        dateTime={freshness.sourceFactWatermark}
                    >
                        {formatDateTime(freshness.sourceFactWatermark, "full")}
                    </time>
                </span>
                {freshness.balanceSnapshotAt ? (
                    <>
                        <span aria-hidden>·</span>
                        <span>
                            余额快照{" "}
                            <time
                                className="num"
                                dateTime={freshness.balanceSnapshotAt}
                            >
                                {formatDateTime(
                                    freshness.balanceSnapshotAt,
                                    "full",
                                )}
                            </time>
                            <span className="ml-1">（独立）</span>
                        </span>
                    </>
                ) : null}
                <span aria-hidden>·</span>
                <span
                    className={
                        freshness.lagSeconds > freshness.maxLagSeconds
                            ? "font-medium text-destructive"
                            : "num"
                    }
                >
                    延迟 {freshness.lagSeconds} 秒（上限{" "}
                    {freshness.maxLagSeconds} 秒）
                </span>
            </div>
        </div>
    )
}
