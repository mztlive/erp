"use client"

import type { RefObject } from "react"
import Link from "next/link"
import { ArrowLeftIcon, ExternalLinkIcon } from "lucide-react"

import {
    DocumentHeader,
    GuardedBusinessAction,
    PageHeader,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"

export function SupplierOrderCenterHeader({
    order,
    from,
    sourceId,
    titleRef,
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
    from: string | null
    sourceId: string | null
    titleRef: RefObject<HTMLSpanElement | null>
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
                breadcrumbs={[
                    {
                        id: "list",
                        label: "供应商订单",
                        href: "/supplier-api/orders",
                    },
                    {
                        id: "order",
                        label: (
                            <span
                                ref={titleRef}
                                tabIndex={-1}
                                className="outline-none"
                            >
                                {order.orderNo}
                            </span>
                        ),
                        current: true,
                    },
                ]}
                actions={
                    from === "mall-order" && sourceId ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={`/commerce/consumption-orders?q=${encodeURIComponent(order.mallOrderNo)}`}
                                />
                            }
                        >
                            <ArrowLeftIcon className="size-3.5" />
                            返回商城订单
                        </Button>
                    ) : (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href="/supplier-api/orders" />}
                        >
                            <ArrowLeftIcon className="size-3.5" />
                            返回列表
                        </Button>
                    )
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
                        商城单 {order.mallOrderNo}
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
                            查询原结果
                        </GuardedBusinessAction>
                    ) : undefined
                }
                secondaryActions={
                    <div className="flex flex-wrap gap-2">
                        {isResultUnknown ? (
                            <GuardedBusinessAction
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
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            render={
                                <Link
                                    href={`/commerce/consumption-orders?q=${encodeURIComponent(order.mallOrderNo)}`}
                                />
                            }
                        >
                            商城 {order.mallOrderNo}
                        </Button>
                    </div>
                }
            />
        </>
    )
}
