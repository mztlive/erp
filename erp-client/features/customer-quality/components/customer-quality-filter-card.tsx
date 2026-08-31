"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import type { BusinessTypeFilter, FundsReviewFilter } from "../types"
import type {
    CustomerQualityAppliedChip,
    CustomerQualityFilterKey,
} from "../hooks/use-customer-quality-filters"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const FUNDS_REVIEW_OPTIONS: ReadonlyArray<{
    value: FundsReviewFilter
    label: string
}> = [
    { value: "all", label: "全部授权记录" },
    { value: "reviewed_only", label: "仅已复核卡券票款" },
]

const BUSINESS_TYPE_OPTIONS: ReadonlyArray<{
    value: BusinessTypeFilter | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "VOUCHER", label: "卡券" },
    { value: "GOODS_SERVICE", label: "非卡券" },
]

export type CustomerQualityFilterCardProps = {
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    appliedChips: readonly CustomerQualityAppliedChip[]
    onRemoveFilter: (key: CustomerQualityFilterKey) => void
    onApplyFilters: () => void
    onClearAllFilters: () => void
    onResetMoreFilters: () => void
    fundsReviewDraft: FundsReviewFilter
    setFundsReviewDraft: SetState<FundsReviewFilter>
    businessTypeDraft: BusinessTypeFilter | "all"
    setBusinessTypeDraft: SetState<BusinessTypeFilter | "all">
}

/**
 * 客户经营质量明细筛选工具栏（docs/ui-filter-design.md §8 公司商品池结构）：
 * 整个筛选区是唯一语义 <form>；收起态搜索框尾部提交箭头与展开态面板
 * 「应用全部筛选」走同一个 onApplyFilters；已生效条件统一进入 chip 行。
 */
export function CustomerQualityFilterCard({
    searchDraft,
    onSearchDraftChange,
    searchInputRef,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    appliedChips,
    onRemoveFilter,
    onApplyFilters,
    onClearAllFilters,
    onResetMoreFilters,
    fundsReviewDraft,
    setFundsReviewDraft,
    businessTypeDraft,
    setBusinessTypeDraft,
}: CustomerQualityFilterCardProps) {
    const panelId = React.useId()
    const hasChips = appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onApplyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup className="w-full">
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="customers-quality-search"
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                onSearchDraftChange(event.target.value)
                            }
                            placeholder="客户编号 / 名称"
                            aria-label="搜索客户"
                        />
                    </InputGroup>
                }
                filters={
                    <Button
                        id="customers-quality-more-filters-trigger"
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
                                            id={`customers-quality-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            key={chip.key}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                onRemoveFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="customers-quality-clear-all"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={onClearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="客户经营质量更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        id="customers-quality-funds-review"
                                        label="票款口径"
                                        value={fundsReviewDraft}
                                        onValueChange={setFundsReviewDraft}
                                        options={FUNDS_REVIEW_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        id="customers-quality-business-type"
                                        label="业务性质"
                                        value={businessTypeDraft}
                                        onValueChange={setBusinessTypeDraft}
                                        options={BUSINESS_TYPE_OPTIONS}
                                    />
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="customers-quality-reset-more"
                                                type="button"
                                                variant="ghost"
                                                onClick={onResetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="customers-quality-apply-filters"
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
    )
}
