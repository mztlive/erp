/**
 * W22 商品发布 · 列表筛选的用户可见文案与选项。
 * 用户可见词遵守 docs/ui-glossary.md（投递 → 发送）。
 */

import type { PublicationDeliveryStatusSelection } from "@/features/product-publications/hooks/use-publication-list-filters"
import type { PublicationStatus } from "@/features/product-publications/types"
import { PUBLICATION_STATUS_LABEL } from "@/features/product-publications/types"

export const PUBLICATION_DELIVERY_STATUS_FILTER_LABELS: Record<
    Exclude<PublicationDeliveryStatusSelection, "all">,
    string
> = {
    pending_confirm: "待商城确认",
    failed: "失败",
    handoff: "转人工",
    acked: "已确认",
}

export const PUBLICATION_DELIVERY_STATUS_RADIO_FILTER_OPTIONS: ReadonlyArray<{
    value: PublicationDeliveryStatusSelection
    label: string
}> = [
    { value: "all", label: "全部" },
    ...(
        Object.keys(
            PUBLICATION_DELIVERY_STATUS_FILTER_LABELS,
        ) as Array<Exclude<PublicationDeliveryStatusSelection, "all">>
    ).map((value) => ({
        value,
        label: PUBLICATION_DELIVERY_STATUS_FILTER_LABELS[value],
    })),
]

export const PUBLICATION_METRIC_FILTER_LABELS: Record<string, string> = {
    pending_publish: "待发布",
    pending_confirm: "待商城确认",
    failed_handoff: "失败/转人工",
    mall_live: "商城已生效",
    paused: "已暂停",
}

export const PUBLICATION_STATUS_FILTER_OPTIONS: ReadonlyArray<{
    value: PublicationStatus | "all"
    label: string
}> = [
    { value: "all", label: "有效发布" },
    ...(Object.keys(PUBLICATION_STATUS_LABEL) as PublicationStatus[]).map(
        (value) => ({ value, label: PUBLICATION_STATUS_LABEL[value] }),
    ),
]
