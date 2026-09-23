"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type {
    BatchFilterKey,
    BatchObjectTypeDraft,
} from "@/features/import-opening/hooks/use-batch-list-filters"
import {
    OBJECT_CODE_LABEL,
    type ImportObjectCode,
} from "@/features/import-opening/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type BatchAppliedChip = Readonly<{
    key: BatchFilterKey
    label: string
}>

const OBJECT_FILTER_OPTIONS: ReadonlyArray<{
    value: BatchObjectTypeDraft
    label: string
}> = [
    { value: "all", label: "全部对象" },
    ...(Object.keys(OBJECT_CODE_LABEL) as ImportObjectCode[]).map((code) => ({
        value: code,
        label: OBJECT_CODE_LABEL[code],
    })),
]

const toolbarIdPrefix = "operations-import-batches-toolbar"

export function BatchListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    appliedChips,
    removeFilter,
    applyBatchFilters,
    objectTypeDraft,
    setObjectTypeDraft,
    clearAllFilters,
    hasPendingChanges = false,
    resultCount,
    loading,
    failed,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    appliedChips: readonly BatchAppliedChip[]
    removeFilter: (key: BatchFilterKey) => void
    applyBatchFilters: () => void
    objectTypeDraft: BatchObjectTypeDraft
    setObjectTypeDraft: SetState<BatchObjectTypeDraft>
    clearAllFilters: () => void
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            density="compact"
            idPrefix={`${toolbarIdPrefix}-filter`}
            formAriaLabel="导入批次查询"
            onSubmit={applyBatchFilters}
            search={
                <ListSearchField
                    id={`${toolbarIdPrefix}-search`}
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="搜索批次号"
                    aria-label="搜索批次"
                />
            }
            queryButtonId={`${toolbarIdPrefix}-apply-filters`}
            primaryFilters={
                <OptionCombobox
                    id={`${toolbarIdPrefix}-object-type`}
                    className="w-56 max-w-full min-w-0"
                    filterLabel="对象集合"
                    value={objectTypeDraft}
                    onValueChange={(value) =>
                        setObjectTypeDraft(
                            (value ?? "all") as BatchObjectTypeDraft,
                        )
                    }
                    options={OBJECT_FILTER_OPTIONS}
                    aria-label="对象集合"
                    placeholder="全部对象"
                    searchPlaceholder="搜索对象名称"
                    allowClear={false}
                />
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "个批次",
                loadingLabel: "正在加载批次…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as BatchFilterKey)}
            onClearAll={clearAllFilters}
            clearButtonId={`${toolbarIdPrefix}-clear-all`}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
        />
    )
}
