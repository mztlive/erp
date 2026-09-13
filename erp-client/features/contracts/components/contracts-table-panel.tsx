"use client"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"

import { FileUpIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    OptionCombobox,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    useContractsList,
    type ContractFilterKey,
} from "@/features/contracts/hooks/use-contracts-list"
import type { ContractListRow } from "@/features/contracts/types"

type ContractsTablePanelProps = {
    list: ReturnType<typeof useContractsList>
    columns: ColumnDef<ContractListRow>[]
    isError: boolean
    error: Error | null
    isPending: boolean
    onRetry: () => void
    onOpenUpload: () => void
    onPreview: (contractId: string) => void
    highlightedContractId?: string
}

/** 合同列表筛选工具栏与数据表。 */
export function ContractsTablePanel({
    list,
    columns,
    isError,
    error,
    isPending,
    onRetry,
    onOpenUpload,
    onPreview,
    highlightedContractId,
}: ContractsTablePanelProps) {
    const {
        searchDraft,
        setSearchDraft,
        searchInputRef,
        panelOpen,
        setPanelOpen,
        settlementPartyIdDraft,
        setSettlementPartyIdDraft,
        ownerDraft,
        setOwnerDraft,
        applyFilters,
        resetMoreFilters,
        hasPendingChanges,
        removeFilter,
        clearAllFilters,
        appliedChips,
        isFiltered,
        settlementPartyOptions,
        ownerOptions,
        pageRows,
        total,
        sorting,
        pagination,
        handleSortingChange,
        handlePaginationChange,
    } = list

    const moreCount = appliedChips.filter(({ key }) =>
        ["settlementPartyId", "owner"].includes(key),
    ).length

    return (
        <ListWorkSurface
            ariaLabel="合同列表"
            toolbar={
                <ListWorkspaceFilterBar
                    idPrefix="card-contracts-list"
                    formAriaLabel="合同查询"
                    onSubmit={applyFilters}
                    queryButtonId="card-contracts-list-apply-filters"
                    moreButtonId="card-contracts-list-more-filters-trigger"
                    clearButtonId="card-contracts-list-clear-all"
                    search={
                        <ListSearchField
                            id="card-contracts-list-search"
                            searchInputRef={searchInputRef}
                            value={searchDraft}
                            onChange={setSearchDraft}
                            placeholder="合同号、客户、结算主体、负责人"
                            aria-label="搜索合同"
                        />
                    }
                    moreCount={moreCount}
                    moreOpen={panelOpen}
                    onToggleMore={() => setPanelOpen((open) => !open)}
                    morePanelId="card-contracts-list-more-panel"
                    morePanelAriaLabel="合同更多筛选条件"
                    onResetMore={resetMoreFilters}
                    morePanel={
                        <div className="grid min-w-0 gap-5 sm:grid-cols-2">
                            <ListWorkspaceFilterField
                                htmlFor="card-contracts-list-filter-settlement-party"
                                label="结算主体"
                            >
                                <OptionCombobox
                                    id="card-contracts-list-filter-settlement-party"
                                    className="w-full"
                                    value={settlementPartyIdDraft}
                                    aria-label="结算主体"
                                    onValueChange={setSettlementPartyIdDraft}
                                    options={settlementPartyOptions}
                                    placeholder="全部结算主体"
                                    searchPlaceholder="搜索结算主体名称"
                                />
                            </ListWorkspaceFilterField>
                            <ResponsibleUserFilter
                                id="card-contracts-list-filter-owner"
                                label="当前跟进负责人"
                                value={ownerDraft ?? ""}
                                onChange={setOwnerDraft}
                                options={ownerOptions}
                            />
                        </div>
                    }
                    resultStatus={listWorkspaceFilterStatusText({
                        loading: isPending,
                        failed: isError,
                        resultCount: isPending ? undefined : total,
                        noun: "份合同",
                        loadingLabel: "正在加载合同…",
                    })}
                    chips={appliedChips}
                    onClearChip={(key) =>
                        removeFilter(key as ContractFilterKey)
                    }
                    onClearAll={clearAllFilters}
                    hasPendingChanges={hasPendingChanges}
                    pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                    idleHint="导出与当前查询结果一致"
                />
            }
            table={
                <DataTable<ContractListRow>
                    id="card-contracts-list-table"
                    data={pageRows}
                    columns={columns}
                    defaultColumnVisibility={{ revision: false }}
                    getRowId={(row) => row.contractId}
                    rowCount={total}
                    sorting={sorting}
                    onSortingChange={handleSortingChange}
                    pagination={pagination}
                    onPaginationChange={handlePaginationChange}
                    loading={isPending}
                    layout="flush"
                    defaultColumnPinning={{
                        left: ["contractNo"],
                        right: ["actions"],
                    }}
                    errorState={
                        isError ? (
                            <BusinessFailureState
                                id="card-contracts-list-failure"
                                title="合同列表加载失败"
                                error={error}
                                onRetry={onRetry}
                            />
                        ) : undefined
                    }
                    emptyState={
                        !isError && pageRows.length === 0 && !isPending ? (
                            <BusinessEmptyState
                                kind={isFiltered ? "filter" : "no-data"}
                                className={listWorkspaceEmptyStateClassName}
                                title={isFiltered ? undefined : "还没有合同"}
                                description={
                                    isFiltered
                                        ? "换一个关键词或清除筛选后再试。"
                                        : "上传第一份合同 PDF，即可用于新建销售单。"
                                }
                                action={
                                    isFiltered ? (
                                        <Button
                                            id="card-contracts-list-empty-clear"
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            onClick={clearAllFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : (
                                        <Button
                                            id="card-contracts-list-empty-upload"
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            onClick={onOpenUpload}
                                        >
                                            <FileUpIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            上传合同 PDF
                                        </Button>
                                    )
                                }
                            />
                        ) : undefined
                    }
                    onRowPreview={(row) => onPreview(row.contractId)}
                    onRowOpen={(row) => onPreview(row.contractId)}
                    highlightedRowId={highlightedContractId}
                />
            }
        />
    )
}
