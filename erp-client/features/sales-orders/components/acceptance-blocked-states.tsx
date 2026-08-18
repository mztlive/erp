"use client"

import Link from "next/link"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"

export function AcceptanceBlockedState({
    isCard,
    blockerMessage,
    salesOrderId,
}: {
    isCard: boolean
    blockerMessage?: string
    salesOrderId: string
}) {
    return (
        <BusinessEmptyState
            kind="no-data"
            title={isCard ? "卡券单不用做客户验收" : "当前不能验收"}
            description={blockerMessage ?? "请确认本单类型与你的权限后再试。"}
            action={
                <Button
                    render={<Link href={`/sales/orders/${salesOrderId}`} />}
                    variant="outline"
                >
                    返回本单
                </Button>
            }
        />
    )
}

export function AcceptanceNoFactsState() {
    return (
        <BusinessEmptyState
            kind="no-data"
            title="还没有可验收的交付记录"
            description="请先完成收货/发货或服务交付登记；也可以查看历史验收。"
            action={
                <Button
                    /* 销售只读，没有归属岗位：不带 lane，走中性页头 */
                    render={<Link href="/fulfillment" />}
                    variant="outline"
                >
                    去发货/交付
                </Button>
            }
        />
    )
}
