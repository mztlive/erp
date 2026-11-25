"use client"

import {
    ChevronLeftIcon,
    ChevronRightIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"

import { StatusBadge } from "@/components/ui/status-badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    OptionCombobox,
    type ComboboxOption,
} from "@/components/ui/option-combobox"
import { cn } from "@/lib/utils"

import { detailWorkspaceStyles } from "../detail-workspace-styles"

export const APPROVAL_TABLE_FILTER_THRESHOLD = 8
export const APPROVAL_TABLE_DEFAULT_PAGE_SIZE = 20

const PAGE_SIZE_OPTIONS: readonly ComboboxOption[] = [
    { value: "20", label: "20 项" },
    { value: "50", label: "50 项" },
]

export type ApprovalLineFilterBarProps = Readonly<{
    idPrefix: string
    totalCount: number
    filteredCount: number
    riskCount: number
    hasHighRisk: boolean
    riskOnly: boolean
    onRiskOnlyChange: (next: boolean) => void
    search: string
    onSearchChange: (next: string) => void
    onClearFilters: () => void
    searchPlaceholder?: string
    ariaLabel?: string
}>

/**
 * 销售单审批与采购单审批共用的明细筛选工具栏：
 * - 统一触发阈值（>8 行、存在风险项或处于筛选态）；
 * - 统一分段切换（「全部」/「仅看风险项」）、搜索框与筛选结果清除入口。
 */
export function ApprovalLineFilterBar({
    idPrefix,
    totalCount,
    filteredCount,
    riskCount,
    hasHighRisk,
    riskOnly,
    onRiskOnlyChange,
    search,
    onSearchChange,
    onClearFilters,
    searchPlaceholder = "搜索商品、规格或收货人",
    ariaLabel = "明细筛选",
}: ApprovalLineFilterBarProps) {
    const hasActiveFilters = Boolean(search.trim()) || riskOnly
    const showToolbar =
        totalCount > APPROVAL_TABLE_FILTER_THRESHOLD ||
        riskCount > 0 ||
        hasActiveFilters

    if (!showToolbar) return null

    return (
        <div
            data-slot="approval-line-filter-bar"
            className="flex flex-wrap items-center justify-between gap-2.5"
            aria-label={ariaLabel}
        >
            <div className="flex flex-wrap items-center gap-2">
                <div
                    role="group"
                    aria-label="风险筛选"
                    className={detailWorkspaceStyles.subTabs}
                >
                    <button
                        id={`${idPrefix}-filter-all`}
                        type="button"
                        aria-pressed={!riskOnly}
                        data-active={!riskOnly ? "true" : undefined}
                        className={detailWorkspaceStyles.subTab}
                        onClick={() => onRiskOnlyChange(false)}
                    >
                        <span>全部</span>
                        <span className="num text-xs text-muted-foreground">
                            {totalCount}
                        </span>
                    </button>
                    {riskCount > 0 || riskOnly ? (
                        <button
                            id={`${idPrefix}-filter-risk`}
                            type="button"
                            aria-pressed={riskOnly}
                            data-active={riskOnly ? "true" : undefined}
                            className={detailWorkspaceStyles.subTab}
                            onClick={() => onRiskOnlyChange(true)}
                        >
                            <span>仅看风险项</span>
                            <StatusBadge
                                tone={
                                    hasHighRisk
                                        ? "destructive"
                                        : riskCount > 0
                                          ? "warning"
                                          : "neutral"
                                }
                                label={String(riskCount)}
                                className="num rounded-full px-1.5 py-0 text-tiny"
                            />
                        </button>
                    ) : null}
                </div>

                {hasActiveFilters ? (
                    <div
                        className="flex items-center gap-1.5 text-xs text-muted-foreground"
                        role="status"
                        aria-live="polite"
                    >
                        <span className="num">
                            命中 {filteredCount} / {totalCount} 项
                        </span>
                        <Button
                            id={`${idPrefix}-clear-filters`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    </div>
                ) : null}
            </div>

            <div className="relative w-full sm:w-60">
                <SearchIcon
                    aria-hidden="true"
                    className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                />
                <Input
                    id={`${idPrefix}-search`}
                    size="sm"
                    value={search}
                    onChange={(event) => onSearchChange(event.target.value)}
                    placeholder={searchPlaceholder}
                    aria-label={searchPlaceholder}
                    className="pr-8 pl-8 text-xs"
                />
                {search ? (
                    <Button
                        id={`${idPrefix}-search-clear`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="清除搜索"
                        className="absolute top-1/2 right-0.5 -translate-y-1/2"
                        onClick={() => onSearchChange("")}
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                ) : null}
            </div>
        </div>
    )
}

export type ApprovalLinePaginationProps = Readonly<{
    idPrefix: string
    filteredCount: number
    page: number
    pageSize: number
    onPageChange: (nextPage: number) => void
    onPageSizeChange: (nextPageSize: number) => void
    ariaLabel?: string
}>

/**
 * 销售单审批与采购单审批共用的分页控件：
 * 仅在多页或切换页容量时渲染，统一支持 20 / 50 项与页码直达。
 */
export function ApprovalLinePagination({
    idPrefix,
    filteredCount,
    page,
    pageSize,
    onPageChange,
    onPageSizeChange,
    ariaLabel = "明细分页",
}: ApprovalLinePaginationProps) {
    const showPagination =
        filteredCount > APPROVAL_TABLE_DEFAULT_PAGE_SIZE ||
        pageSize !== APPROVAL_TABLE_DEFAULT_PAGE_SIZE

    if (!showPagination) return null

    const pageCount = Math.max(1, Math.ceil(filteredCount / pageSize))
    const currentPage = Math.min(page, pageCount)
    const firstPage = Math.max(1, Math.min(currentPage - 2, pageCount - 4))
    const pageNumbers = Array.from(
        { length: Math.min(5, pageCount) },
        (_, index) => firstPage + index,
    )

    return (
        <div
            data-slot="approval-line-pagination"
            className="flex flex-wrap items-center justify-between gap-3 pt-1"
        >
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <label
                    id={`${idPrefix}-page-size-label`}
                    htmlFor={`${idPrefix}-page-size`}
                >
                    每页
                </label>
                <OptionCombobox
                    id={`${idPrefix}-page-size`}
                    size="sm"
                    value={String(pageSize)}
                    onValueChange={(next) => {
                        if (!next) return
                        onPageSizeChange(next === "50" ? 50 : 20)
                    }}
                    options={PAGE_SIZE_OPTIONS}
                    getOptionId={(value) => `${idPrefix}-page-size-${value}`}
                    allowClear={false}
                    aria-label="每页显示条数"
                    className="w-24"
                />
                <span className="num">
                    第 {currentPage} / {pageCount} 页 · 共 {filteredCount} 项
                </span>
            </div>

            <nav aria-label={ariaLabel} className="flex items-center gap-1">
                <Button
                    id={`${idPrefix}-page-prev`}
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="上一页明细"
                    disabled={currentPage === 1}
                    onClick={() => onPageChange(currentPage - 1)}
                >
                    <ChevronLeftIcon aria-hidden="true" />
                </Button>
                {pageNumbers.map((value) => (
                    <Button
                        id={`${idPrefix}-page-${value}`}
                        key={value}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`第 ${value} 页明细`}
                        aria-current={
                            value === currentPage ? "page" : undefined
                        }
                        className={cn(
                            "num",
                            value === currentPage &&
                                "border-info-border bg-info-soft text-info-soft-foreground",
                        )}
                        onClick={() => onPageChange(value)}
                    >
                        {value}
                    </Button>
                ))}
                <Button
                    id={`${idPrefix}-page-next`}
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="下一页明细"
                    disabled={currentPage === pageCount}
                    onClick={() => onPageChange(currentPage + 1)}
                >
                    <ChevronRightIcon aria-hidden="true" />
                </Button>
            </nav>
        </div>
    )
}

/** 审批明细筛选与分页控件组合出口（对齐任务页设计系统 §7.1 命名）。 */
export const WorkspaceApprovalTableControls = {
    FilterBar: ApprovalLineFilterBar,
    Pagination: ApprovalLinePagination,
    FILTER_THRESHOLD: APPROVAL_TABLE_FILTER_THRESHOLD,
    DEFAULT_PAGE_SIZE: APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
} as const
