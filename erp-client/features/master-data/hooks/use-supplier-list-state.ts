"use client"

import * as React from "react"

import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useClientPagedRows } from "@/features/master-data/hooks/use-client-paged-rows"
import { useSupplierListFilters } from "@/features/master-data/hooks/use-supplier-list-filters"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useMasterDataListExport } from "@/features/master-data/hooks/use-master-data-list-export"
import { buildSupplierFilterSnapshotLabel } from "@/features/master-data/lib/master-data-list-summaries"
import { syncListMetrics } from "@/features/master-data/lib/master-data-list-summaries"
import { resourceLabel } from "@/features/master-data/lib/data"
import type { MasterDataListItem } from "@/features/master-data/types"

export function useSupplierListState(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const { canCreate, createBlockedReason } =
        useCreatePermission("supplier:create")
    const filters = useSupplierListFilters(searchInputRef)
    const listQuery = useMasterDataListQuery({
        resource: "suppliers",
        q: filters.q.trim() || undefined,
        lifecycleStatus: filters.lifecycleStatus,
        supplierCapabilityCodes: filters.supplierCapabilityCodes,
        supplierQualificationTypes: filters.supplierQualificationTypes,
        supplierQualificationHealth: filters.supplierQualificationHealth,
    })
    const { exportMeta, handleExport } = useMasterDataListExport()
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const pageRows = useClientPagedRows(rows, filters.pagination)
    const syncedMetrics = React.useMemo(() => {
        const base = listQuery.data?.metrics ?? []
        if (rows.length === 0 || listQuery.data == null) return base
        return syncListMetrics(base, rows).filter(
            (metric) => metric.key !== "pending",
        )
    }, [listQuery.data, rows])
    const filterSnapshotLabel = React.useMemo(
        () =>
            buildSupplierFilterSnapshotLabel({
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                supplierCapabilityCodes: filters.supplierCapabilityCodes,
                supplierQualificationTypes: filters.supplierQualificationTypes,
                supplierQualificationHealth: filters.supplierQualificationHealth,
            }),
        [
            filters.lifecycleStatus,
            filters.q,
            filters.supplierCapabilityCodes,
            filters.supplierQualificationHealth,
            filters.supplierQualificationTypes,
        ],
    )

    const onExport = React.useCallback(() => {
        if (!listQuery.data || rows.length === 0) return
        void handleExport(
            {
                resource: "suppliers",
                q: filters.q.trim() || undefined,
                lifecycleStatus: filters.lifecycleStatus,
                supplierCapabilityCodes: filters.supplierCapabilityCodes,
                supplierQualificationTypes: filters.supplierQualificationTypes,
                supplierQualificationHealth: filters.supplierQualificationHealth,
            },
            filterSnapshotLabel,
            resourceLabel("suppliers"),
        )
    }, [
        filterSnapshotLabel,
        filters,
        handleExport,
        listQuery.data,
        rows.length,
    ])

    return {
        filters,
        listQuery,
        exportMeta,
        canCreate,
        createBlockedReason,
        disableTarget,
        setDisableTarget,
        rows,
        pageRows,
        syncedMetrics,
        onExport,
    }
}
