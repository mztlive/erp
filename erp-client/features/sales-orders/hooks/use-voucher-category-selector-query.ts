"use client"

import { useQuery } from "@tanstack/react-query"

import { fetchMasterDataList } from "@/features/master-data/api"

const STALE_TIME = 5 * 60 * 1000

/** 卡券类目优先读取正式档案；未配置档案时回退可销售卡券 SKU。 */
export function useVoucherCategorySelectorQuery(purpose: string) {
    return useQuery({
        queryKey: ["sales-orders", "voucher-category", { purpose }],
        queryFn: async () => {
            const profiles = await fetchMasterDataList({
                resource: "voucher-categories",
                lifecycleStatus: "enabled",
            })
            const source =
                profiles.rows.length > 0
                    ? profiles.rows
                    : (
                          await fetchMasterDataList({
                              resource: "sellable-items",
                              lifecycleStatus: "enabled",
                              productKind: "VOUCHER",
                          })
                      ).rows
            return source.map((item) => ({
                productId: item.stableId,
                revisionId: item.currentRevisionId,
                sku: item.stableNo,
                name: item.name,
                statusLabel: item.lifecycleStatusLabel,
                statusTone: item.lifecycleTone,
                baseUnit: "张",
                description:
                    item.keyFacts.find((fact) => fact.label === "说明")
                        ?.value ??
                    item.keyFacts.find((fact) => fact.label === "商品类型")
                        ?.value,
            }))
        },
        staleTime: STALE_TIME,
    })
}
