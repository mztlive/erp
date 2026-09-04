"use client"

import Link from "next/link"
import { ArrowLeftIcon, ExternalLinkIcon, Loader2Icon } from "lucide-react"

import {
    DocumentHeader,
    GuardedBusinessAction,
    PageHeader,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"

export function SupplierOrderCenterHeader({
    order,
    canQuery,
    canReplay,
    isResultUnknown,
    actionBlockers,
    allowedActions,
    queryPending,
    replayPending,
    onQueryResult,
    onReplayClick,
}: {
    order: SupplierOrderDetailView["order"]
    canQuery: boolean
    canReplay: boolean
    isResultUnknown: boolean
    actionBlockers: SupplierOrderDetailView["actionBlockers"]
    allowedActions: string[]
    queryPending: boolean
    replayPending: boolean
    onQueryResult: () => void
    onReplayClick: () => void
}) {
    return (
        <>
            <PageHeader
                variant="object-chrome"
                actions={
                    <Button
                        id="supplier-order-center-header-back-list"
                        type="button"
                        size="sm"
                        variant="outline"
                        render={<Link href="/supplier-api/orders" />}
                    >
                        <ArrowLeftIcon className="size-3.5" />
                        返回列表
                    </Button>
                }
            />

            <DocumentHeader
                density="compact"
                title={order.supplierName}
                documentNumber={order.orderNo}
                primaryStatus={{
                    label: order.fulfillmentLabel,
                    tone: order.fulfillmentTone,
                }}
                meta={
                    <span className="text-muted-foreground">
                        供应商单号 {order.externalOrderNo ?? "尚未返回"}
                    </span>
                }
                statuses={[
                    {
                        id: "supplier",
                        label: "供应商",
                        status: { label: order.supplierName, tone: "neutral" },
                    },
                    {
                        id: "external",
                        label: "外部单号",
                        status: {
                            label: order.externalOrderNo ?? "尚未返回",
                            tone: order.externalOrderNo ? "info" : "neutral",
                        },
                    },
                ]}
                primaryAction={
                    isResultUnknown ? (
                        <GuardedBusinessAction
                            id="supplier-order-center-header-query-result"
                            type="button"
                            size="sm"
                            disabled={!canQuery || queryPending}
                            reason={
                                !canQuery
                                    ? (actionBlockers.find(
                                          (b) => b.action === "QUERY_RESULT",
                                      )?.message ?? "当前不可查询")
                                    : undefined
                            }
                            onClick={onQueryResult}
                        >
                            {queryPending ? (
                                <Loader2Icon
                                    data-icon="inline-start"
                                    className="size-4 animate-spin"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {queryPending ? "查询中…" : "查询原结果"}
                        </GuardedBusinessAction>
                    ) : undefined
                }
                secondaryActions={
                    <div className="flex flex-wrap gap-2">
                        {isResultUnknown ? (
                            <GuardedBusinessAction
                                id="supplier-order-center-header-replay"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={!canReplay || replayPending}
                                reason={
                                    canReplay
                                        ? "已确认无结果，可安全重发"
                                        : (actionBlockers.find(
                                              (b) => b.action === "REPLAY",
                                          )?.message ??
                                          "需先查询确认无结果后，方可重发")
                                }
                                onClick={onReplayClick}
                            >
                                安全重发
                            </GuardedBusinessAction>
                        ) : null}
                        {allowedActions.includes("ESCALATE_W29") ? (
                            <Button
                                id="supplier-order-center-header-escalate"
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link href="/governance/integration-errors" />
                                }
                            >
                                转接口错误中心
                                <ExternalLinkIcon className="size-3.5" />
                            </Button>
                        ) : null}
                        <span className="num text-sm text-muted-foreground">
                            供应商单号 {order.externalOrderNo ?? "尚未返回"}
                        </span>
                    </div>
                }
            />
        </>
    )
}
