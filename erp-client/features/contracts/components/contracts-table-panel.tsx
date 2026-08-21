"use client"

import { FileUpIcon, SearchIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    FilterChip,
    ListToolbar,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
import { contractMetricLabel } from "@/features/contracts/lib/filter-contracts"
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
}

/** 合同列表区：工具栏 + 空态/失败态 + 数据表。 */
export function ContractsTablePanel({
    list,
    columns,
    isError,
    error,
    isPending,
    onRetry,
    onOpenUpload,
    onPreview,
}: ContractsTablePanelProps) {
    const {
        q,
        metric,
        searchDraft,
        setSearchDraft,
        lockedCustomer,
        handleClearCustomerLock,
        handleSearchCommit,
        clearAllFilters,
        isFiltered,
        pageRows,
        sorted,
        sorting,
        pagination,
        handleSortingChange,
        handlePaginationChange,
    } = list

    return (
        <BusinessTableFrame
            title="合同列表"
            description={
                metric === "all" && !(q ?? "").trim()
                    ? "按将到期优先排序展示当前业务范围内的合同。"
                    : `当前筛选：${contractMetricLabel(metric)}${
                          (q ?? "").trim() ? ` · “${(q ?? "").trim()}”` : ""
                      }`
            }
            toolbar={
                <ListToolbar
                    search={
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                data-slot="contracts-search"
                                value={searchDraft}
                                onChange={(event) => {
                                    setSearchDraft(event.target.value)
                                }}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                        handleSearchCommit(searchDraft)
                                    }
                                }}
                                placeholder="合同号、客户、结算主体、负责人"
                                aria-label="搜索合同"
                            />
                        </InputGroup>
                    }
                    secondary={
                        lockedCustomer ? (
                            <FilterChip
                                label={`客户：${lockedCustomer.displayName}`}
                                onClear={handleClearCustomerLock}
                                clearLabel="清除客户锁定"
                            />
                        ) : undefined
                    }
                    actions={
                        <div className="flex items-center gap-2 text-xs text-muted-foreground">
                            <span aria-live="polite">
                                共 {sorted.length.toLocaleString("zh-CN")} 条
                            </span>
                            {isFiltered ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={clearAllFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : null}
                        </div>
                    }
                />
            }
            table={
                isError ? (
                    <BusinessFailureState
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
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={clearAllFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : (
                                <Button
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
                    />
                )
            }
        />
    )
}
