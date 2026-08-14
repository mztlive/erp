/** 仓库对象中心：修订时间线 + W10 库存摘要链接 + 敏感字段占位。 */

import type {
    WarehouseDto,
    WarehouseRevisionDto,
} from "@/features/master-data/api/contracts"
import { mapWarehouseRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import { asLifecycle } from "@/features/master-data/api/presentation"
import type {
    MasterDataCenterView,
    RevisionTimelineEntry,
} from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerWarehouse(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const warehouses = await fetchAllPages<WarehouseDto>(
        "/admin/warehouses",
        {},
    )
    const wh = warehouses.find((w) => w.id === stableId)
    if (!wh) return null
    const revisions = await fetchAllPages<WarehouseRevisionDto>(
        "/admin/warehouse-revisions",
        { warehouse_id: stableId, sort_by: "revision_no", sort_dir: "desc" },
    )
    const current = revisions[0]
    const row = mapWarehouseRow(wh, current)
    const timeline: RevisionTimelineEntry[] = revisions.map((r, index) => ({
        id: r.id,
        revisionNo: r.revision_no,
        revisionTiming:
            index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
        timingLabel: index === 0 ? "当前生效" : "已结束",
        nameSnapshot: r.name,
        actor: "—",
        effectiveFrom: r.effective_from,
        effectiveTo: r.effective_to ?? undefined,
        changeReason: r.change_reason,
        isCurrent: index === 0,
        lifecycleAtRevision: asLifecycle(wh.status),
    }))
    return baseCenter("warehouses", row, {
        warehouseStockSummary: {
            onHandQty: "—",
            reservedQty: "—",
            hasBlockingStock: false,
            w10Href: `/inventory?warehouseId=${encodeURIComponent(wh.id)}`,
            policyNote: "库存摘要由 W10 投影提供；当前接口未返回数量。",
        },
        revisionTimeline:
            timeline.length > 0
                ? timeline
                : baseCenter("warehouses", row).revisionTimeline,
        sensitiveFields: [
            {
                label: "联系人 / 地址",
                maskedValue: "（敏感字段，需授权查看）",
                visibility: "masked",
            },
        ],
    })
}
