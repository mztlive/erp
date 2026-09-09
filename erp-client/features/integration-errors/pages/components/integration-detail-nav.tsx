import { DataFreshness, DetailPageHeader } from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"
import { RefreshCwIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { IntegrationView } from "../../types"

export function IntegrationDetailNav({
    itemNumber,
    updatedAt,
    view,
    queueContextId,
    onRefresh,
}: {
    itemNumber?: string
    updatedAt?: string
    view: IntegrationView
    queueContextId: string
    onRefresh: () => void
}) {
    return (
        <DetailPageHeader
            title={itemNumber ?? "接口错误与对账中心"}
            back={{
                id: "integration-detail-nav-back",
                label: "返回队列",
                href: `/governance/integration-errors?view=${view}&queueContextId=${encodeURIComponent(queueContextId)}`,
            }}
            meta={
                <DataFreshness
                    state="fresh"
                    label={freshnessText.dataUpdatedAt}
                    updatedAt={formatDateTime(updatedAt, "default")}
                    dateTime={updatedAt}
                />
            }
            primaryAction={
                <Button
                    id="integration-detail-nav-refresh"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="text-muted-foreground"
                    onClick={onRefresh}
                >
                    <RefreshCwIcon data-icon="inline-start" aria-hidden />
                    刷新当前任务
                </Button>
            }
        />
    )
}
