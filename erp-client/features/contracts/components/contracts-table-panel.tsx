"use client"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"

import { SettlementPartySearchCombobox } from "@/features/party-selector/settlement-party-search-combobox"

import { FileUpIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
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
        orgDraft,
        setOrgDraft,
        descendantsDraft,
        setDescendantsDraft,
        applyFilters,
        resetMoreFilters,
        cancelMoreFilters,
        hasPendingChanges,
        removeFilter,
        clearAllFilters,
        appliedChips,
        isFiltered,

        pageRows,
        total,
        sorting,
        pagination,
        handleSortingChange,
        handlePaginationChange,
    } = list

    const moreCount = appliedChips.filter(
        ({ key }) => key === "orgUnitIds",
    ).length

    return (
        <ListWorkSurface
            ariaLabel="合同列表"
            toolbar={
                <ListWorkspaceFilterBar
                    morePresentation="popover"
                    moreSize="compact"
                    className="[&_[data-slot=list-toolbar-search]]:lg:w-80 [&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
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
                    onToggleMore={() =>
                        panelOpen ? cancelMoreFilters() : setPanelOpen(true)
                    }
                    morePanelId="card-contracts-list-more-panel"
                    morePanelAriaLabel="合同更多筛选条件"
                    onResetMore={resetMoreFilters}
                    primaryFilters={
                        <>
                            <SettlementPartySearchCombobox
                                purpose="filter"
                                id="card-contracts-list-filter-settlement-party"
                                className="w-48 max-w-full"
                                value={settlementPartyIdDraft ?? undefined}
                                aria-label="结算主体"
                                filterLabel="结算主体"
                                onValueChange={(id) =>
                                    setSettlementPartyIdDraft(id ?? null)
                                }
                                placeholder="全部"
                            />
                            <div className="w-56 max-w-full">
                                <PersonDirectoryFilter
                                    id="card-contracts-list-filter-owner"
                                    label="当前跟进负责人"
                                    hideLabel
                                    value={ownerDraft ?? ""}
                                    onChange={setOwnerDraft}
                                    category="sales"
                                />
                            </div>
                        </>
                    }
                    morePanel={
                        <OrganizationUnitFilter
                            id="card-contracts-list-filter-org"
                            value={orgDraft}
                            onChange={setOrgDraft}
                            includeDescendants={descendantsDraft}
                            onDescendantsChange={setDescendantsDraft}
                        />
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
                        !isError &&
                        pageRows.length === 0 &&
                        !isPending &&
                        list.contractsQuery.data?.emptyReason === "no_scope" ? (
                            <BusinessEmptyState
                                kind="no-scope"
                                className={listWorkspaceEmptyStateClassName}
                                title="当前角色无合同范围"
                                description="当前权限与数据范围内没有合同；不代表系统尚无合同。"
                            />
                        ) : !isError && pageRows.length === 0 && !isPending ? (
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
