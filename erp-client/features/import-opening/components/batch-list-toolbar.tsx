"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type {
    BatchFilterKey,
    BatchObjectTypeDraft,
    BatchStatusDraft,
} from "@/features/import-opening/hooks/use-batch-list-filters"
import {
    BATCH_STATUS_LABEL,
    OBJECT_CODE_LABEL,
    type ImportBatchStatus,
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

const STATUS_FILTER_OPTIONS: ReadonlyArray<{
    value: BatchStatusDraft
    label: string
}> = [
    { value: "all", label: "全部状态" },
    ...(Object.keys(BATCH_STATUS_LABEL) as ImportBatchStatus[]).map(
        (status) => ({
            value: status,
            label: BATCH_STATUS_LABEL[status],
        }),
    ),
]

const toolbarIdPrefix = "operations-import-batches-toolbar"

export function BatchListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    appliedChips,
    removeFilter,
    batchFilterPanelOpen,
    setBatchFilterPanelOpen,
    applyBatchFilters,
    resetMoreFilters,
    objectTypeDraft,
    setObjectTypeDraft,
    statusDraft,
    setStatusDraft,
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
    batchFilterPanelOpen: boolean
    setBatchFilterPanelOpen: SetState<boolean>
    applyBatchFilters: () => void
    resetMoreFilters: () => void
    objectTypeDraft: BatchObjectTypeDraft
    setObjectTypeDraft: SetState<BatchObjectTypeDraft>
    statusDraft: BatchStatusDraft
    setStatusDraft: SetState<BatchStatusDraft>
    clearAllFilters: () => void
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    const moreCount = appliedChips.filter(({ key }) => key === "status").length

    return (
        <ListWorkspaceFilterBar
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
                    placeholder="批次号（精确/前缀匹配）"
                    aria-label="搜索批次"
                />
            }
            queryButtonId={`${toolbarIdPrefix}-apply-filters`}
            moreCount={moreCount}
            moreOpen={batchFilterPanelOpen}
            onToggleMore={() => setBatchFilterPanelOpen((open) => !open)}
            moreButtonId={`${toolbarIdPrefix}-filter-trigger`}
            morePanelId={`${toolbarIdPrefix}-more-panel`}
            morePanelAriaLabel="导入批次更多筛选条件"
            onResetMore={resetMoreFilters}
            resetMoreButtonId={`${toolbarIdPrefix}-reset-filters`}
            commonFilters={
                <ListWorkspaceInlineFilter
                    htmlFor={`${toolbarIdPrefix}-object-type`}
                    label="对象集合"
                >
                    <OptionCombobox
                        id={`${toolbarIdPrefix}-object-type`}
                        className="w-full sm:w-60"
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
                </ListWorkspaceInlineFilter>
            }
            morePanel={
                <ListWorkspaceFilterField
                    htmlFor={`${toolbarIdPrefix}-status`}
                    label="批次状态"
                >
                    <OptionCombobox
                        id={`${toolbarIdPrefix}-status`}
                        className="w-full sm:w-60"
                        value={statusDraft}
                        onValueChange={(value) =>
                            setStatusDraft((value ?? "all") as BatchStatusDraft)
                        }
                        options={STATUS_FILTER_OPTIONS}
                        aria-label="批次状态"
                        placeholder="全部状态"
                        searchPlaceholder="搜索状态名称"
                        allowClear={false}
                    />
                </ListWorkspaceFilterField>
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
