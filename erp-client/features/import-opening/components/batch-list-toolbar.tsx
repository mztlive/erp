"use client"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { listFilterText } from "@/lib/ui-text"
import type {
    BatchObjectTypeDraft,
    useBatchListFilters,
} from "@/features/import-opening/hooks/use-batch-list-filters"
import {
    OBJECT_CODE_LABEL,
    type ImportObjectCode,
} from "@/features/import-opening/types"

const OBJECT_FILTER_OPTIONS = [
    { value: "all", label: "全部类型" },
    ...(Object.keys(OBJECT_CODE_LABEL) as ImportObjectCode[]).map((code) => ({
        value: code,
        label: OBJECT_CODE_LABEL[code],
    })),
]

const toolbarIdPrefix = "operations-import-batches-toolbar"

export function BatchListToolbar({
    filters,
    loading,
    failed,
}: {
    filters: ReturnType<typeof useBatchListFilters>
    loading: boolean
    failed: boolean
}) {
    const {
        searchInputRef,
        qDraft,
        setQDraft,
        objectTypeDraft,
        setObjectTypeDraft,
        applyBatchFilters,
        clearAllBatchFilters,
        removeBatchFilter,
        hasPendingChanges,
        hasAppliedBatchFilters,
        appliedChips,
    } = filters

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            idPrefix={`${toolbarIdPrefix}-filter`}
            formAriaLabel="导入批次查询"
            onSubmit={applyBatchFilters}
            search={
                <ListSearchField
                    id={`${toolbarIdPrefix}-search`}
                    searchInputRef={searchInputRef}
                    value={qDraft}
                    onChange={setQDraft}
                    placeholder="搜索批次号"
                    aria-label="搜索批次"
                />
            }
            queryButtonId={`${toolbarIdPrefix}-apply-filters`}
            primaryFilters={
                <OptionCombobox
                    id={`${toolbarIdPrefix}-object-type`}
                    className="w-56 max-w-full min-w-0"
                    filterLabel="导入数据类型"
                    value={objectTypeDraft === "all" ? null : objectTypeDraft}
                    onValueChange={(value) =>
                        setObjectTypeDraft(
                            (value ?? "all") as BatchObjectTypeDraft,
                        )
                    }
                    options={OBJECT_FILTER_OPTIONS}
                    aria-label="导入数据类型"
                    placeholder="全部"
                    allowClear={false}
                />
            }
            resultStatus={
                failed || loading
                    ? listWorkspaceFilterStatusText({
                          failed,
                          loading: loading && !failed,
                          noun: "批次",
                      })
                    : null
            }
            chips={appliedChips.map((chip) => ({
                ...chip,
                onClear: () => removeBatchFilter(chip.key),
            }))}
            extraPrimary={
                hasAppliedBatchFilters ? (
                    <Button
                        id={`${toolbarIdPrefix}-clear-all`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="font-normal text-muted-foreground"
                        onClick={clearAllBatchFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : null
            }
            hasPendingChanges={hasPendingChanges}
        />
    )
}
