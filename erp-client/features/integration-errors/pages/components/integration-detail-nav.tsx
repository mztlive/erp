import type * as React from "react"
import { RefreshCwIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { integrationStatusTone } from "../../lib/presentation"
import type {
    IntegrationItemType,
    IntegrationResolutionItemView,
    IntegrationView,
} from "../../types"

export function IntegrationDetailNav({
    item,
    defaultItemType = "ERROR_TASK",
    updatedAt,
    view,
    queueContextId,
    returnTo,
    headingRef,
    onRefresh,
}: {
    item?: IntegrationResolutionItemView
    defaultItemType?: IntegrationItemType
    updatedAt?: string
    view: IntegrationView
    queueContextId: string
    returnTo?: string
    headingRef?: React.RefObject<HTMLHeadingElement | null>
    onRefresh: () => void
}) {
    const itemType = item?.identity.itemType ?? defaultItemType
    const isTask = itemType === "ERROR_TASK"
    const fallbackTitle = isTask ? "外部同步异常" : "对账差异"
    const backHref =
        returnTo ??
        `/governance/integration-errors?view=${view}&queueContextId=${encodeURIComponent(queueContextId)}`

    return (
        <DetailWorkspaceChrome
            title={item?.businessObject.title || fallbackTitle}
            headingProps={
                headingRef
                    ? {
                          id: "integration-item-title",
                          ref: headingRef,
                          tabIndex: -1,
                          className: "outline-none",
                      }
                    : undefined
            }
            documentNumber={item?.identity.number || undefined}
            numberLabel={isTask ? "异常任务号" : "差异编号"}
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
                href: backHref,
                visibility: returnTo ? "always" : "standalone-only",
            }}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        item
                            ? `处理人：${item.ownerUser ?? item.ownerRole}`
                            : false,
                        item ? (
                            <span key="age">
                                已滞留{" "}
                                <span className="num">{item.ageLabel}</span>
                            </span>
                        ) : (
                            false
                        ),
                        item ? item.fundsImpactLabel : false,
                        item?.environment
                            ? `${item.environmentLabel}环境`
                            : false,
                        updatedAt ? (
                            <span key="updated">
                                更新于{" "}
                                <time className="num" dateTime={updatedAt}>
                                    {formatDateTime(updatedAt, "default")}
                                </time>
                            </span>
                        ) : (
                            false
                        ),
                    ]}
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
