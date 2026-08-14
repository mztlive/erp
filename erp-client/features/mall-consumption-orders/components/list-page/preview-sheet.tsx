"use client"

import Link from "next/link"

import {
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConsumptionOrderPreviewPanel } from "@/features/mall-consumption-orders/components/consumption-order-preview-panel"
import { useConsumptionOrderDetailQuery } from "@/features/mall-consumption-orders/hooks/queries"
import { previewDataSourceLabel } from "@/features/mall-consumption-orders/lib/labels"
import {
    ATTRIBUTION_STATUS_LABEL,
    ATTRIBUTION_STATUS_TONE,
    FULFILLMENT_CHAIN_LABEL,
    FULFILLMENT_CHAIN_TONE,
} from "@/features/mall-consumption-orders/types"

type Props = {
    previewId: string | null
    onClose: () => void
    listReturnHref: string
}

export function ConsumptionOrderPreviewSheet({
    previewId,
    onClose,
    listReturnHref,
}: Props) {
    const previewQuery = useConsumptionOrderDetailQuery(previewId)

    return (
        <QuickPreviewSheet
            open={Boolean(previewId)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            size="detail"
            title={
                previewQuery.data?.identity.externalOrderNo ??
                "商城消费订单预览"
            }
            identity={
                previewQuery.data ? (
                    <span className="num">
                        {previewQuery.data.identity.mallOrderId}
                        <span className="mx-1">·</span>
                        {previewQuery.data.identity.mallName}
                    </span>
                ) : null
            }
            summary={
                previewQuery.data ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={
                                FULFILLMENT_CHAIN_LABEL[
                                    previewQuery.data.fulfillment.chain
                                ]
                            }
                            tone={
                                FULFILLMENT_CHAIN_TONE[
                                    previewQuery.data.fulfillment.chain
                                ]
                            }
                        />
                        <BusinessStatusBadge
                            context="preview"
                            label={
                                ATTRIBUTION_STATUS_LABEL[
                                    previewQuery.data.customer.attributionStatus
                                ]
                            }
                            tone={
                                ATTRIBUTION_STATUS_TONE[
                                    previewQuery.data.customer.attributionStatus
                                ]
                            }
                        />
                        <Badge variant="secondary">
                            {previewDataSourceLabel(previewQuery.data)}
                        </Badge>
                    </div>
                ) : null
            }
            footer={
                previewQuery.data ? (
                    <>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            render={
                                <Link
                                    href={`/commerce/consumption-orders/${previewQuery.data.identity.mallOrderId}?section=overview&returnTo=${encodeURIComponent(listReturnHref)}`}
                                />
                            }
                        >
                            打开中心
                        </Button>
                    </>
                ) : null
            }
        >
            {previewQuery.isPending ? (
                <div className="p-5 text-sm text-muted-foreground">
                    加载预览…
                </div>
            ) : previewQuery.data ? (
                <ConsumptionOrderPreviewPanel view={previewQuery.data} />
            ) : (
                <div className="p-5 text-sm text-muted-foreground">
                    未找到该消费订单
                </div>
            )}
        </QuickPreviewSheet>
    )
}
