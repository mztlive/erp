"use client"

import * as React from "react"
import Link from "next/link"
import { PackageIcon } from "lucide-react"

import {
    DocumentSection,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import { RelatedLanes } from "@/features/sales-orders/components/sales-order-detail-related-lanes"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { fulfillmentWorkspaceHref } from "@/features/sales-orders/lib/sales-order-detail-model"
import { cn } from "@/lib/utils"

function AcceptanceSummary({
    order,
    canAccept,
    expanded,
    onExpand,
    onCollapse,
}: {
    order: SalesOrderDetailView
    canAccept: boolean
    expanded: boolean
    onExpand: () => void
    onCollapse: () => void
}) {
    const latest = order.acceptance

    if (expanded) {
        return (
            <div className="space-y-3">
                <div className="flex items-center justify-between gap-2">
                    <h3 className="text-sm font-medium">客户验收</h3>
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={onCollapse}
                    >
                        收起验收
                    </Button>
                </div>
                <AcceptanceWorkspace salesOrderId={order.id} />
            </div>
        )
    }

    return (
        <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <h3 className="text-sm font-medium">客户验收</h3>
                    <p className="text-xs text-muted-foreground">
                        {latest
                            ? `最近 ${latest.reference} · ${latest.postedAt}${latest.note ? ` · ${latest.note}` : ""}`
                            : "还没有验收记录。客户确认完成后，本单才算交付完毕。"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        交付进度：{order.fulfillment.label}
                    </p>
                </div>
                <Button
                    type="button"
                    size="sm"
                    disabled={!canAccept}
                    title={
                        canAccept
                            ? undefined
                            : "当前不能验收，请先完成交付或确认权限。"
                    }
                    onClick={onExpand}
                >
                    登记验收
                </Button>
            </div>
        </div>
    )
}

export function FulfillmentPanel({
    order,
    selfReturn,
    acceptanceExpanded,
    canAccept,
    onExpandAcceptance,
    onCollapseAcceptance,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    acceptanceExpanded: boolean
    canAccept: boolean
    onExpandAcceptance: () => void
    onCollapseAcceptance: () => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="space-y-3">
            <SectionLead>
                {isCard
                    ? "卡券到期即算交付完成。消费多少不影响本单是否交付完毕。"
                    : "采购接单和发货在对应工作面完成；客户确认后，在本页登记验收。"}
            </SectionLead>
            <DocumentSection
                title="采购与交付"
                className="py-3 first:pt-0 last:pb-0"
                action={
                    isCard ? undefined : (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={fulfillmentWorkspaceHref(
                                        order,
                                        selfReturn,
                                    )}
                                />
                            }
                        >
                            <PackageIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            去发货/交付
                        </Button>
                    )
                }
            >
                <RelatedLanes
                    order={order}
                    selfReturn={selfReturn}
                    lanes={
                        isCard ? ["fulfillment"] : ["purchase", "fulfillment"]
                    }
                />
            </DocumentSection>

            {isCard ? (
                <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                    <h3 className="text-sm font-medium">卡券交付</h3>
                    <p className="mt-1 text-xs text-muted-foreground">
                        到期即算交付完成。期限{" "}
                        {order.fulfillmentDeadline || "—"} · 当前{" "}
                        {order.fulfillment.label}
                        。消费多少不影响本单是否交付完成。
                    </p>
                </div>
            ) : (
                <AcceptanceSummary
                    order={order}
                    canAccept={canAccept}
                    expanded={acceptanceExpanded}
                    onExpand={onExpandAcceptance}
                    onCollapse={onCollapseAcceptance}
                />
            )}
        </div>
    )
}
