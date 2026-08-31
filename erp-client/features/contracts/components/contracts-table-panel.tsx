"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FileUpIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    FilterChip,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
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

/**
 * 合同列表区：筛选 form（ListToolbar + 更多筛选面板 + 已筛选 chip 行）+ 空态/失败态 + 数据表。
 * 结构契约见 docs/ui-filter-design.md §1.2 / §3 / §8.2。
 */
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
        hasStructuredFilters,
        settlementPartyIdDraft,
        setSettlementPartyIdDraft,
        ownerDraft,
        setOwnerDraft,
        applyFilters,
        resetMoreFilters,
        removeFilter,
        clearAllFilters,
        appliedChips,
        isFiltered,
        filterDescription,
        settlementPartyOptions,
        ownerOptions,
        pageRows,
        sorted,
        sorting,
        pagination,
        handleSortingChange,
        handlePaginationChange,
    } = list

    const panelId = React.useId()
    const hasChips = appliedChips.length > 0

    return (
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    合同列表
                    <span
                        aria-live="polite"
                        className="font-normal text-muted-foreground"
                    >
                        {sorted.length.toLocaleString("zh-CN")} 条
                    </span>
                </span>
            }
            description={filterDescription}
            toolbar={
                <form
                    onSubmit={(event) => {
                        event.preventDefault()
                        applyFilters()
                    }}
                >
                    <ListToolbar
                        search={
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    id="card-contracts-list-search"
                                    ref={searchInputRef}
                                    value={searchDraft}
                                    onChange={(event) => {
                                        setSearchDraft(event.target.value)
                                    }}
                                    placeholder="合同号、客户、结算主体、负责人"
                                    aria-label="搜索合同"
                                />
                            </InputGroup>
                        }
                        filters={
                            <Button
                                id="card-contracts-list-more-filters-trigger"
                                type="button"
                                variant="outline"
                                aria-expanded={panelOpen}
                                aria-controls={panelId}
                                onClick={() => setPanelOpen((open) => !open)}
                            >
                                <FilterIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                更多筛选
                                {hasStructuredFilters ? (
                                    <Badge variant="info">已启用</Badge>
                                ) : null}
                                <ChevronDownIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                    className={
                                        panelOpen
                                            ? "rotate-180 transition-transform"
                                            : "transition-transform"
                                    }
                                />
                            </Button>
                        }
                        secondary={
                            hasChips || panelOpen ? (
                                <div className="w-full space-y-3">
                                    {hasChips ? (
                                        <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                            <span className="text-xs text-muted-foreground">
                                                已筛选
                                            </span>
                                            {appliedChips.map((chip) => (
                                                <FilterChip
                                                    key={chip.key}
                                                    id={`card-contracts-list-filter-chip-${chip.key}`}
                                                    label={chip.label}
                                                    clearLabel={`移除${chip.label}`}
                                                    onClear={() =>
                                                        removeFilter(chip.key)
                                                    }
                                                />
                                            ))}
                                            <Button
                                                id="card-contracts-list-clear-all"
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                onClick={clearAllFilters}
                                            >
                                                清空全部
                                            </Button>
                                        </div>
                                    ) : null}
                                    {panelOpen ? (
                                        <div
                                            id={panelId}
                                            className="flex w-full flex-col gap-3 border-t pt-3"
                                            aria-label="合同更多筛选条件"
                                        >
                                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                    <span className="text-muted-foreground">
                                                        结算主体
                                                    </span>
                                                    <OptionCombobox
                                                        id="card-contracts-list-filter-settlement-party"
                                                        className="w-full"
                                                        value={
                                                            settlementPartyIdDraft
                                                        }
                                                        aria-label="结算主体"
                                                        onValueChange={
                                                            setSettlementPartyIdDraft
                                                        }
                                                        options={
                                                            settlementPartyOptions
                                                        }
                                                        placeholder="全部结算主体"
                                                        searchPlaceholder="搜索结算主体名称"
                                                    />
                                                </div>
                                                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                    <span className="text-muted-foreground">
                                                        负责人
                                                    </span>
                                                    <OptionCombobox
                                                        id="card-contracts-list-filter-owner"
                                                        className="w-full"
                                                        value={ownerDraft}
                                                        aria-label="负责人"
                                                        onValueChange={
                                                            setOwnerDraft
                                                        }
                                                        options={ownerOptions}
                                                        placeholder="全部负责人"
                                                        searchPlaceholder="搜索负责人姓名"
                                                    />
                                                </div>
                                            </div>
                                            <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                                <p className="text-xs text-muted-foreground">
                                                    将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                                </p>
                                                <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                                    <Button
                                                        id="card-contracts-list-reset-more"
                                                        type="button"
                                                        variant="ghost"
                                                        onClick={
                                                            resetMoreFilters
                                                        }
                                                    >
                                                        重置更多条件
                                                    </Button>
                                                    <Button
                                                        id="card-contracts-list-apply-filters"
                                                        type="submit"
                                                    >
                                                        <SearchIcon
                                                            data-icon="inline-start"
                                                            aria-hidden="true"
                                                        />
                                                        应用全部筛选
                                                    </Button>
                                                </div>
                                            </div>
                                        </div>
                                    ) : null}
                                </div>
                            ) : undefined
                        }
                    />
                </form>
            }
            table={
                isError ? (
                    <BusinessFailureState
                        id="card-contracts-list-failure"
                        title="合同列表加载失败"
                        error={error}
                        onRetry={onRetry}
                    />
                ) : pageRows.length === 0 && !isPending ? (
                    <BusinessEmptyState
                        kind={isFiltered ? "filter" : "no-data"}
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
                ) : (
                    <DataTable<ContractListRow>
                        id="card-contracts-list-table"
                        data={pageRows}
                        columns={columns}
                        getRowId={(row) => row.contractId}
                        rowCount={sorted.length}
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
                        onRowPreview={(row) => onPreview(row.contractId)}
                        onRowOpen={(row) => onPreview(row.contractId)}
                        highlightedRowId={highlightedContractId}
                    />
                )
            }
        />
    )
}
