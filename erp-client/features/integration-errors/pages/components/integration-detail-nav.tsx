import {
    DataFreshness,
    DetailWorkspaceChrome,
    detailWorkspaceStyles,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"
import { RefreshCwIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { integrationStatusTone } from "../../lib/presentation"
import type {
    IntegrationResolutionItemView,
    IntegrationView,
} from "../../types"

export function IntegrationDetailNav({
    item,
    updatedAt,
    view,
    queueContextId,
    onRefresh,
}: {
    item?: IntegrationResolutionItemView
    updatedAt?: string
    view: IntegrationView
    queueContextId: string
    onRefresh: () => void
}) {
    return (
        <DetailWorkspaceChrome
            title={item?.identity.number ?? "外部同步异常与对账"}
            primaryStatus={
                item
                    ? {
                          label: item.status.label,
                          tone: integrationStatusTone(item),
                      }
                    : undefined
            }
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
            secondaryActions={
                <Button
                    id="integration-detail-nav-refresh"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={onRefresh}
                >
                    <RefreshCwIcon data-icon="inline-start" aria-hidden />
                    刷新当前任务
                </Button>
            }
        />
    )
}
