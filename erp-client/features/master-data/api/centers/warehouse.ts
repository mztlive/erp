/** 仓库对象中心：修订时间线 + W10 库存台账直达入口。 */

import type {
    WarehouseDto,
    WarehouseRevisionDto,
} from "@/features/master-data/api/contracts"
import { mapWarehouseRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import { asLifecycle } from "@/features/master-data/api/presentation"
import { fetchWarehouseFulfillmentHandlerOptions } from "@/features/master-data/api/warehouse-fulfillment"
import type {
    MasterDataCenterView,
    RevisionTimelineEntry,
} from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerWarehouse(
    stableId: string,
    canReadRevisions = true,
    canReadHandlerOptions = false,
): Promise<MasterDataCenterView | null> {
    const warehouses = await fetchAllPages<WarehouseDto>(
        "/admin/warehouses",
        {},
    )
    const wh = warehouses.find((w) => w.id === stableId)
    if (!wh) return null

    const [revisions, handlerOptions] = await Promise.all([
        canReadRevisions
            ? fetchAllPages<WarehouseRevisionDto>(
                  "/admin/warehouse-revisions",
                  {
                      warehouse_id: stableId,
                      sort_by: "revision_no",
                      sort_dir: "desc",
                  },
              )
            : Promise.resolve([] as WarehouseRevisionDto[]),
        canReadHandlerOptions &&
        (wh.inbound_handler_user_id || wh.outbound_handler_user_id)
            ? fetchWarehouseFulfillmentHandlerOptions().catch(() => [])
            : Promise.resolve([]),
    ])

    const handlerLabelById = new Map(
        handlerOptions.map((option) => [
            option.user_id,
            `${option.display_name}（${option.account}）`,
        ]),
    )
    const handlerLabels = {
        inbound: wh.inbound_handler_user_id
            ? handlerLabelById.get(wh.inbound_handler_user_id)
            : undefined,
        outbound: wh.outbound_handler_user_id
            ? handlerLabelById.get(wh.outbound_handler_user_id)
            : undefined,
    }

    const current = revisions[0]
    const row = mapWarehouseRow(wh, current, handlerLabels)
    const overviewFacts = [
        {
            label: "入库经办人",
            value: wh.inbound_handler_user_id
                ? (handlerLabels.inbound ?? "已配置")
                : "未配置",
        },
        {
            label: "仓发经办人",
            value: wh.outbound_handler_user_id
                ? (handlerLabels.outbound ?? "已配置")
                : "未配置",
        },
    ]

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
    const base = baseCenter("warehouses", row)
    return baseCenter("warehouses", row, {
        currentRevision: {
            ...base.currentRevision,
            changeReason: current?.change_reason?.trim() || "—",
            fields: overviewFacts,
        },
        resourceFacts: overviewFacts,
        warehouseFulfillmentHandlers: row.warehouseFulfillmentHandlers,
        warehouseStockSummary: {
            onHandQty: "—",
            reservedQty: "—",
            hasBlockingStock: false,
            w10Href: `/inventory?warehouseId=${encodeURIComponent(wh.id)}`,
            policyNote: "可前往库存台账查看该仓库的实时在库与预占明细。",
        },
        revisionTimeline:
            timeline.length > 0 ? timeline : base.revisionTimeline,
    })
}
