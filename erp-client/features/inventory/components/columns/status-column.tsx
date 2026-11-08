"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"

type InventoryStatusRow = {
    statusLabel: string
    statusTone: "success" | "destructive" | "warning" | "info" | "neutral"
}

/** 库存列表共用的状态列。文案和色调来自行投影，不在列里再映射。 */
export function inventoryStatusColumn<
    T extends InventoryStatusRow,
>(): ColumnDef<T> {
    return {
        id: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
            <BusinessStatusBadge
                context="list"
                label={row.original.statusLabel}
                tone={row.original.statusTone}
            />
        ),
    }
}
