"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { FileUpIcon } from "lucide-react"

import { BusinessFailureState, OptionCombobox } from "@/components/business"
import {
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import type { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
import { contractMetricLabel } from "@/features/contracts/lib/filter-contracts"
import type { ContractListRow } from "@/features/contracts/types"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { SettlementPartySearchCombobox } from "@/features/party-selector/settlement-party-search-combobox"
import { listFilterText } from "@/lib/ui-text"

const METRIC_OPTIONS = (
    ["effective", "expiring_30d", "expired", "terminated"] as const
).map((value) => ({ value, label: contractMetricLabel(value) }))

const DEFAULT_COLUMN_ORDER = [
    "contractNo",
    "settlement",
    "validity",
    "status",
    "owner",
    "sales",
    "revision",
    "actions",
]

const PAGE_SIZE_OPTIONS = [20, 50, 100]

type ContractsTablePanelProps = {
    list: ReturnType<typeof useContractsList>
    columns: ColumnDef<ContractListRow>[]
    canReadDetail: boolean
    onOpenUpload?: () => void
    onPreview: (contractId: string) => void
    highlightedContractId?: string
}

export function ContractsTablePanel({
    list,
    columns,
    canReadDetail,
    onOpenUpload,
    onPreview,
    highlightedContractId,
}: ContractsTablePanelProps) {
    const {
        canReadList,
        canFilterOwners,
        canFilterOrgs,
        contractsQuery,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        metricDraft,
        setMetricDraft,
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

    const { error, isError, isFetching } = contractsQuery
    const failed = isError || (!isFetching && !contractsQuery.data)
    const retry = () => void contractsQuery.refetch()
    const preview = (row: ContractListRow) => onPreview(row.contractId)
    const pageSizeOptions = PAGE_SIZE_OPTIONS.includes(pagination.pageSize)
        ? PAGE_SIZE_OPTIONS
        : [...PAGE_SIZE_OPTIONS, pagination.pageSize].sort((a, b) => a - b)

    if (!canReadList) {
        return (
            <BusinessFailureState
                id="card-contracts-list-failure"
                kind="permission"
                title="无法查看合同列表"
            />
        )
    }

    return (
        <ListWorkSurface
            ariaLabel="合同列表"
            toolbar={
                <ListWorkspaceFilterBar
                    morePresentation="popover"
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
                    moreCount={
                        appliedChips.some(({ key }) => key === "orgUnitIds")
                            ? 1
                            : 0
                    }
                    moreOpen={panelOpen}
                    onToggleMore={() =>
                        panelOpen ? cancelMoreFilters() : setPanelOpen(true)
                    }
                    morePanelId="card-contracts-list-more-panel"
                    morePanelAriaLabel="合同更多筛选条件"
                    onResetMore={resetMoreFilters}
                    primaryFilters={
                        <>
                            <OptionCombobox
                                id="card-contracts-list-filter-metric"
                                className="w-36 max-w-full min-w-0"
                                options={METRIC_OPTIONS}
                                value={
                                    metricDraft === "all" ? null : metricDraft
                                }
                                onValueChange={(value) =>
                                    setMetricDraft(
                                        METRIC_OPTIONS.find(
                                            (option) => option.value === value,
                                        )?.value ?? "all",
                                    )
                                }
                                filterLabel="状态"
                                aria-label="合同状态"
                                placeholder="全部"
                            />
                            <div
                                className="w-44 max-w-full min-w-0 empty:hidden"
                                data-has-value={
                                    settlementPartyIdDraft ? "true" : undefined
                                }
                            >
                                <SettlementPartySearchCombobox
                                    purpose="filter"
                                    id="card-contracts-list-filter-settlement-party"
                                    value={settlementPartyIdDraft ?? undefined}
                                    aria-label="结算主体"
                                    filterLabel="结算主体"
                                    onValueChange={(id) =>
                                        setSettlementPartyIdDraft(id ?? null)
                                    }
                                    placeholder="全部"
                                />
                            </div>
                            {canFilterOwners && (
                                <div className="w-56 max-w-full min-w-0 empty:hidden">
                                    <PersonDirectoryFilter
                                        id="card-contracts-list-filter-owner"
                                        label="当前跟进负责人"
                                        hideLabel
                                        value={ownerDraft ?? ""}
                                        onChange={setOwnerDraft}
                                        category="sales"
                                    />
                                </div>
                            )}
                        </>
                    }
                    extraPrimary={
                        isFiltered && appliedChips.length === 0 ? (
                            <Button
                                id="card-contracts-list-clear-all"
                                type="button"
                                variant="ghost"
                                className={listWorkspaceStyles.quietButton}
                                onClick={clearAllFilters}
                            >
                                {listFilterText.clear}
                            </Button>
                        ) : undefined
                    }
                    morePanel={
                        canFilterOrgs ? (
                            <ListFilterSections>
                                <ListFilterSection legend="数据范围">
                                    <div className="min-w-0 md:col-span-2">
                                        <OrganizationUnitFilter
                                            id="card-contracts-list-filter-org"
                                            value={orgDraft}
                                            onChange={setOrgDraft}
                                            includeDescendants={
                                                descendantsDraft
                                            }
                                            onDescendantsChange={
                                                setDescendantsDraft
                                            }
                                        />
                                    </div>
                                </ListFilterSection>
                            </ListFilterSections>
                        ) : null
                    }
                    resultStatus={
                        failed ? "查询未完成" : isFetching ? "查询中…" : null
                    }
                    chips={appliedChips}
                    onClearChip={(key) => {
                        if (key === "customerId" || key === "orgUnitIds")
                            removeFilter(key)
                    }}
                    onClearAll={clearAllFilters}
                    hasPendingChanges={hasPendingChanges}
                    pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                />
            }
            table={
                <ListDataTable
                    id="card-contracts-list-table"
                    caption="合同列表"
                    entityName="合同"
                    data={pageRows}
                    columns={columns}
                    defaultColumnVisibility={{ revision: false }}
                    defaultColumnOrder={DEFAULT_COLUMN_ORDER}
                    defaultColumnPinning={{
                        left: ["contractNo"],
                        right: canReadDetail ? ["actions"] : [],
                    }}
                    getRowId={(row) => row.contractId}
                    rowLabel={({ contractNo, customer }) =>
                        customer.displayName &&
                        customer.displayName !== customer.customerId
                            ? `${contractNo}（${customer.displayName}）`
                            : contractNo
                    }
                    rowCount={total}
                    sorting={sorting}
                    onSortingChange={handleSortingChange}
                    pagination={pagination}
                    onPaginationChange={
                        contractsQuery.isPlaceholderData
                            ? undefined
                            : handlePaginationChange
                    }
                    pageSizeOptions={pageSizeOptions}
                    scrollable
                    showPagination={!failed && (!isFetching || total > 0)}
                    loading={isFetching && !failed}
                    errorState={
                        failed ? (
                            <BusinessFailureState
                                id="card-contracts-list-failure"
                                title="合同列表加载失败"
                                error={error ?? undefined}
                                onRetry={retry}
                            />
                        ) : undefined
                    }
                    noScope={contractsQuery.data?.emptyReason === "no_scope"}
                    hasActiveFilters={isFiltered}
                    onClearFilters={clearAllFilters}
                    clearFiltersButtonId="card-contracts-list-empty-clear"
                    emptyCopy={{
                        noScopeTitle: "当前角色无合同范围",
                        noScopeDescription:
                            "当前权限与数据范围内没有合同；不代表系统尚无合同。",
                        emptyDescription: onOpenUpload
                            ? "上传第一份合同 PDF，即可用于新建销售单。"
                            : undefined,
                    }}
                    emptyAction={
                        onOpenUpload ? (
                            <Button
                                id="card-contracts-list-empty-upload"
                                type="button"
                                size="sm"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onOpenUpload}
                            >
                                <FileUpIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                上传合同 PDF
                            </Button>
                        ) : undefined
                    }
                    onRowPreview={preview}
                    onRowOpen={preview}
                    highlightedRowId={highlightedContractId}
                />
            }
        />
    )
}
