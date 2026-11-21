"use client"

import { type ColumnDef } from "@tanstack/react-table"
import * as React from "react"

import { BusinessStatusBadge, TableRowActions } from "@/components/business"
import {
    blockerColumn,
    effectivePeriodColumn,
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
    stableNoColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

function handlerColumn(
    kind: "inbound" | "outbound",
): ColumnDef<MasterDataListItem> {
    const label = kind === "inbound" ? "入库经办人" : "仓发经办人"
    return {
        id: `${kind}Handler`,
        header: label,
        meta: { label, width: "status" },
        cell: ({ row }) => {
            const handlers = row.original.warehouseFulfillmentHandlers
            const configured = Boolean(
                kind === "inbound"
                    ? handlers?.inboundUserId
                    : handlers?.outboundUserId,
            )
            return (
                <BusinessStatusBadge
                    context="list"
                    appearance={configured ? "quiet" : "badge"}
                    tone={configured ? "success" : "warning"}
                    label={configured ? "已配置" : "未配置"}
                />
            )
        },
    }
}

export function useWarehouseListColumns({
    lastFocusedRowId,
    rows,
    onPreview,
    onReviseTarget,
    canMaintainHandlers,
    canReadRevisions,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onPreview: (stableId: string) => void
    onReviseTarget: (item: MasterDataListItem) => void
    canMaintainHandlers: boolean
    canReadRevisions: boolean
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            nameColumn({ showNumber: true }),
            handlerColumn("inbound"),
            handlerColumn("outbound"),
            lifecycleColumn(),
            {
                id: "actions",
                header: masterDataCopy.colActions,
                meta: { label: masterDataCopy.colActions, align: "end" },
                cell: ({ row }) => {
                    const item = row.original
                    const segment = toAutomationIdSegment(item.stableId)
                    return (
                        <TableRowActions
                            maxInline={2}
                            moreId={`master-data-warehouse-row-${segment}-more`}
                            moreLabel={`${item.name} 更多操作`}
                            actions={[
                                {
                                    id: `master-data-list-row-${segment}-view`,
                                    label: masterDataCopy.actionView,
                                    onClick: () => {
                                        lastFocusedRowId.current = item.stableId
                                        onPreview(item.stableId)
                                    },
                                },
                                {
                                    id: `master-data-warehouse-row-${segment}-handlers`,
                                    label: "配置收发责任",
                                    hidden: !canMaintainHandlers,
                                    disabled: !item.allowedActions.includes(
                                        "MAINTAIN_FULFILLMENT_HANDLERS",
                                    ),
                                    disabledReason: item.actionBlockers.find(
                                        (blocker) =>
                                            blocker.action ===
                                            "MAINTAIN_FULFILLMENT_HANDLERS",
                                    )?.message,
                                    onClick: () => {
                                        lastFocusedRowId.current = item.stableId
                                        onReviseTarget(item)
                                    },
                                },
                            ]}
                        />
                    )
                },
            },
            stableNoColumn(),
            ...(canReadRevisions
                ? [
                      {
                          ...revisionNoColumn(),
                          meta: {
                              label: masterDataCopy.colVersion,
                              width: "quantity" as const,
                              align: "end" as const,
                              numeric: true,
                          },
                      },
                      revisionTimingColumn(),
                      effectivePeriodColumn(),
                  ]
                : []),
            ...blockerColumn(rows),
        ],
        [
            canMaintainHandlers,
            canReadRevisions,
            lastFocusedRowId,
            onPreview,
            onReviseTarget,
            rows,
        ],
    )
}
