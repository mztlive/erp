"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    SearchIcon,
} from "lucide-react"

import { FilterChip, FixedOptionRadioFilter, ListToolbar } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import type {
    AvailabilityStatusFilter,
    OfferingSourceFilter,
    OfferingStatusFilter,
} from "@/features/supplier-offerings/hooks/use-supplier-offerings-page-state"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
} from "@/features/supplier-offerings/types"

const OFFERING_STATUS_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "ACTIVE", label: OFFERING_STATUS_LABELS.ACTIVE },
    { value: "PAUSED", label: OFFERING_STATUS_LABELS.PAUSED },
    { value: "STOPPED", label: OFFERING_STATUS_LABELS.STOPPED },
] as const

const SOURCE_TYPE_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "MANUAL", label: SOURCE_TYPE_LABELS.MANUAL },
    { value: "EXCEL", label: SOURCE_TYPE_LABELS.EXCEL },
    { value: "API", label: SOURCE_TYPE_LABELS.API },
] as const

const AVAILABILITY_STATUS_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "AVAILABLE", label: AVAILABILITY_STATUS_LABELS.AVAILABLE },
    { value: "UNAVAILABLE", label: AVAILABILITY_STATUS_LABELS.UNAVAILABLE },
    { value: "STOPPED", label: AVAILABILITY_STATUS_LABELS.STOPPED },
    { value: "STALE", label: AVAILABILITY_STATUS_LABELS.STALE },
] as const

export type SupplierOfferingsToolbarProps = {
    searchInputRef: React.Ref<HTMLInputElement>
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    filterPanelOpen: boolean
    onFilterPanelOpenChange: (open: boolean) => void
    hasStructuredFilters: boolean
    skuLocked: boolean
    /** 商品页带入的公司 SKU 编号；用于锁定芯片文案。 */
    lockedSkuNo?: string | null
    onRemoveSkuLock: () => void
    statusDraft: OfferingStatusFilter
    onStatusDraftChange: (value: OfferingStatusFilter) => void
    sourceTypeDraft: OfferingSourceFilter
    onSourceTypeDraftChange: (value: OfferingSourceFilter) => void
    availabilityStatusDraft: AvailabilityStatusFilter
    onAvailabilityStatusDraftChange: (value: AvailabilityStatusFilter) => void
    skuIdDraft: string | null
    onSkuIdDraftChange: (value: string | null) => void
    skuNoDraft: string
    onSkuNoDraftChange: (value: string) => void
    productNoDraft: string
    onProductNoDraftChange: (value: string) => void
    supplierIdDraft: string | null
    onSupplierIdDraftChange: (value: string | null) => void
    total: number
    hasFilters: boolean
    onApplyFilters: () => void
    onClearFilters: () => void
}

/** 供给列表搜索、高级筛选与计数动作工具条。 */
export function SupplierOfferingsToolbar({
    searchInputRef,
    searchDraft,
    onSearchDraftChange,
    filterPanelOpen,
    onFilterPanelOpenChange,
    hasStructuredFilters,
    skuLocked,
    lockedSkuNo,
    onRemoveSkuLock,
    statusDraft,
    onStatusDraftChange,
    sourceTypeDraft,
    onSourceTypeDraftChange,
    availabilityStatusDraft,
    onAvailabilityStatusDraftChange,
    skuIdDraft,
    onSkuIdDraftChange,
    skuNoDraft,
    onSkuNoDraftChange,
    productNoDraft,
    onProductNoDraftChange,
    supplierIdDraft,
    onSupplierIdDraftChange,
    total,
    hasFilters,
    onApplyFilters,
    onClearFilters,
}: SupplierOfferingsToolbarProps) {
    const panelId = React.useId()

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onApplyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                onSearchDraftChange(event.target.value)
                            }
                            placeholder="订货编码、SKU 名称/编号"
                            aria-label="搜索供给"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        {!filterPanelOpen ? (
                            <Button type="submit" size="sm">
                                <SearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                搜索
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            aria-expanded={filterPanelOpen}
                            aria-controls={panelId}
                            onClick={() =>
                                onFilterPanelOpenChange(!filterPanelOpen)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
                            {hasStructuredFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    filterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    filterPanelOpen || skuLocked ? (
                        <div className="flex w-full flex-col gap-2">
                            {skuLocked ? (
                                <div>
                                    <FilterChip
                                        label={
                                            lockedSkuNo
                                                ? `公司 SKU：${lockedSkuNo}`
                                                : "来自商品页的公司 SKU"
                                        }
                                        clearLabel="移除公司 SKU 限定"
                                        onClear={onRemoveSkuLock}
                                    />
                                </div>
                            ) : null}
                            {filterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 rounded-lg border border-border bg-muted/30 px-3 py-3"
                                    aria-label="供应商供给筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="关系状态"
                                        value={statusDraft}
                                        onValueChange={onStatusDraftChange}
                                        options={OFFERING_STATUS_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="登记来源"
                                        value={sourceTypeDraft}
                                        onValueChange={onSourceTypeDraftChange}
                                        options={SOURCE_TYPE_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="当前可供"
                                        value={availabilityStatusDraft}
                                        onValueChange={
                                            onAvailabilityStatusDraftChange
                                        }
                                        options={
                                            AVAILABILITY_STATUS_FILTER_OPTIONS
                                        }
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        {!skuLocked ? (
                                            <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                <span className="text-muted-foreground">
                                                    公司 SKU
                                                </span>
                                                <CompanySkuSearchCombobox
                                                    value={
                                                        skuIdDraft ?? undefined
                                                    }
                                                    onValueChange={(value) =>
                                                        onSkuIdDraftChange(
                                                            value ?? null,
                                                        )
                                                    }
                                                    placeholder="全部公司 SKU"
                                                    className="w-full"
                                                    aria-label="公司 SKU"
                                                />
                                            </div>
                                        ) : null}
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                SKU 编号
                                            </span>
                                            <Input
                                                className="w-full"
                                                value={skuNoDraft}
                                                onChange={(event) =>
                                                    onSkuNoDraftChange(
                                                        event.target.value,
                                                    )
                                                }
                                                autoComplete="off"
                                                placeholder="如 SKU-001"
                                                aria-label="SKU 编号"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                SPU 编号
                                            </span>
                                            <Input
                                                className="w-full"
                                                value={productNoDraft}
                                                onChange={(event) =>
                                                    onProductNoDraftChange(
                                                        event.target.value,
                                                    )
                                                }
                                                autoComplete="off"
                                                placeholder="如 P-1001"
                                                aria-label="SPU 编号"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                供应商
                                            </span>
                                            <SupplierSearchCombobox
                                                value={
                                                    supplierIdDraft ?? undefined
                                                }
                                                onValueChange={(value) =>
                                                    onSupplierIdDraftChange(
                                                        value ?? null,
                                                    )
                                                }
                                                placeholder="全部供应商"
                                                className="w-full"
                                                aria-label="供应商"
                                            />
                                        </div>
                                    </div>
                                    <div className="flex justify-end">
                                        <Button type="submit" size="sm">
                                            <SearchIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            搜索
                                        </Button>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
                actions={
                    <>
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            共 {total} 条
                        </span>
                        {hasFilters ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </>
                }
            />
        </form>
    )
}
