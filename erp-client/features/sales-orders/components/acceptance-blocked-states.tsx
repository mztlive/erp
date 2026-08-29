"use client"

import { BusinessEmptyState } from "@/components/business"

export function AcceptanceBlockedState({
    isCard,
    blockerMessage,
}: {
    isCard: boolean
    blockerMessage?: string
}) {
    return (
        <BusinessEmptyState
            kind="no-data"
            title={isCard ? "卡券单不用做客户验收" : "当前不能验收"}
            description={
                blockerMessage ??
                (isCard
                    ? "卡券履约完成按销售单履约期限到期判断。"
                    : "请确认本单类型与你的权限后再试。")
            }
        />
    )
}

export function AcceptanceNoFactsState() {
    return (
        <BusinessEmptyState
            kind="no-data"
            title="还没有可验收的交付记录"
            description="仓储或采购把货发出、或把电子交付和服务登记完成后，待验会出现在这里。发完后也会回到负责销售的待办。"
        />
    )
}
