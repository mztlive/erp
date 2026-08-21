"use client"

import * as React from "react"

import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useClientPagedRows } from "@/features/master-data/hooks/use-client-paged-rows"
import { useSupplierListFilters } from "@/features/master-data/hooks/use-supplier-list-filters"
import type { SupplierFilterKey } from "@/features/master-data/hooks/use-supplier-list-filters"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useMasterDataListExport } from "@/features/master-data/hooks/use-master-data-list-export"
import {
    buildSupplierFilterSnapshotLabel,
    buildSupplierTableDescription,
} from "@/features/master-data/lib/master-data-list-summaries"
import { syncListMetrics } from "@/features/master-data/lib/master-data-list-summaries"
import { lifecycleFilterLabel } from "@/features/master-data/lib/copy"
import {
    qualificationHealthLabel,
    selectedSupplierOptionLabels,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import { resourceLabel } from "@/features/master-data/lib/data"
import type { MasterDataListItem } from "@/features/master-data/types"

export type SupplierAppliedChip = Readonly<{
    key: SupplierFilterKey
    label: string
}>

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
    /** 所有已生效条件均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly SupplierAppliedChip[]>(() => {
        const chips: SupplierAppliedChip[] = []
        if (filters.q.trim()) {
            chips.push({ key: "q", label: `搜索：${filters.q.trim()}` })
        }
        if (filters.lifecycleStatus !== "all") {
            chips.push({
                key: "lifecycleStatus",
                label: `启停：${lifecycleFilterLabel(filters.lifecycleStatus)}`,
            })
        }
        if (filters.supplierQualificationHealth) {
            chips.push({
                key: "supplierQualificationHealth",
                label: `资质状态：${qualificationHealthLabel(filters.supplierQualificationHealth)}`,
            })
        }
        if (filters.supplierCapabilityCodes.length > 0) {
            chips.push({
                key: "supplierCapabilityCodes",
                label: `供应能力：${selectedSupplierOptionLabels(
                    filters.supplierCapabilityCodes,
                    SUPPLIER_CAPABILITY_OPTIONS,
                ).join("、")}`,
            })
        }
        if (filters.supplierQualificationTypes.length > 0) {
            chips.push({
                key: "supplierQualificationTypes",
                label: `资质类型：${selectedSupplierOptionLabels(
                    filters.supplierQualificationTypes,
                    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
                ).join("、")}`,
            })
        }
        return chips
    }, [
        filters.lifecycleStatus,
        filters.q,
        filters.supplierCapabilityCodes,
        filters.supplierQualificationHealth,
        filters.supplierQualificationTypes,
    ])
    const listTableDescription = React.useMemo(
        () =>
            buildSupplierTableDescription({
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                supplierQualificationHealth:
                    filters.supplierQualificationHealth,
                supplierCapabilityCodes: filters.supplierCapabilityCodes,
                supplierQualificationTypes:
                    filters.supplierQualificationTypes,
            }),
        [
            filters.lifecycleStatus,
            filters.q,
            filters.supplierCapabilityCodes,
            filters.supplierQualificationHealth,
            filters.supplierQualificationTypes,
        ],
    )
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
        appliedChips,
        listTableDescription,
        syncedMetrics,
        onExport,
    }
}
