"use client"

import * as React from "react"

import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useClientPagedRows } from "@/features/master-data/hooks/use-client-paged-rows"
import { useLifecycleListFilters } from "@/features/master-data/hooks/use-lifecycle-list-filters"
import {
    useMasterDataCenterQuery,
    useMasterDataListQuery,
} from "@/features/master-data/hooks/queries"
import { useMasterDataListExport } from "@/features/master-data/hooks/use-master-data-list-export"
import {
    buildDictionaryFilterSnapshotLabel,
    buildListTableDescription,
    syncListMetrics,
} from "@/features/master-data/lib/master-data-list-summaries"
import { resourceLabel } from "@/features/master-data/lib/data"
import type {
    MasterDataListItem,
    MasterDataResource,
} from "@/features/master-data/types"

type DictionaryResource = Extract<
    MasterDataResource,
    "brands" | "unit-of-measures" | "voucher-categories" | "warehouses"
>

export function useDictionaryListState({
    resource,
    createPermission,
    enablePreview = false,
    searchInputRef,
}: {
    resource: DictionaryResource
    createPermission?: string
    enablePreview?: boolean
    searchInputRef: React.RefObject<HTMLInputElement | null>
}) {
    const { canCreate, createBlockedReason } =
        useCreatePermission(createPermission)
    const filters = useLifecycleListFilters(searchInputRef)
    const listQuery = useMasterDataListQuery({
        resource,
        q: filters.q.trim() || undefined,
        lifecycleStatus: filters.lifecycleStatus,
        revisionTiming: filters.revisionTiming,
    })
    const { exportMeta, handleExport } = useMasterDataListExport()
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseTarget, setReviseTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [previewId, setPreviewId] = React.useState<string | null>(null)

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const pageRows = useClientPagedRows(rows, filters.pagination)
    const previewDetailQuery = useMasterDataCenterQuery(
        resource,
        enablePreview ? (previewId ?? "") : "",
    )
    const previewRow = React.useMemo(
        () => rows.find((row) => row.stableId === previewId) ?? null,
        [previewId, rows],
    )
    const syncedMetrics = React.useMemo(() => {
        const base = listQuery.data?.metrics ?? []
        if (rows.length === 0 || listQuery.data == null) return base
        return syncListMetrics(base, rows)
    }, [listQuery.data, rows])
    const label = resourceLabel(resource)
    const filterSnapshotLabel = React.useMemo(
        () =>
            buildDictionaryFilterSnapshotLabel({
                categoryLabel: label,
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
            }),
        [filters.lifecycleStatus, filters.q, filters.revisionTiming, label],
    )
    const listTableDescription = React.useMemo(
        () =>
            buildListTableDescription({
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
            }),
        [
            filters.lifecycleStatus,
            filters.q,
            filters.revisionTiming,
        ],
    )

    const onExport = React.useCallback(() => {
        if (!listQuery.data || rows.length === 0) return
        void handleExport(
            {
                resource,
                q: filters.q.trim() || undefined,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
            },
            filterSnapshotLabel,
            label,
        )
    }, [
        filterSnapshotLabel,
        filters.lifecycleStatus,
        filters.q,
        filters.revisionTiming,
        handleExport,
        label,
        listQuery.data,
        resource,
        rows.length,
    ])

    return {
        filters,
        listQuery,
        exportMeta,
        createOpen,
        setCreateOpen,
        reviseTarget,
        setReviseTarget,
        disableTarget,
        setDisableTarget,
        previewId,
        setPreviewId,
        rows,
        pageRows,
        previewDetailQuery,
        previewRow,
        syncedMetrics,
        filterSnapshotLabel,
        listTableDescription,
        canCreate,
        createBlockedReason,
        label,
        onExport,
    }
}
