"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
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
import { MALLS } from "@/features/product-publications/api/publications"
import type {
    PublicationDeliveryStatusSelection,
    PublicationFilterKey,
} from "@/features/product-publications/hooks/use-publication-list-filters"
import type { PublicationStatus } from "@/features/product-publications/types"
import {
    PUBLICATION_DELIVERY_STATUS_RADIO_FILTER_OPTIONS,
    PUBLICATION_STATUS_FILTER_OPTIONS,
} from "@/features/product-publications/lib/publication-filter-labels"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type PublicationAppliedChip = Readonly<{
    key: PublicationFilterKey
    label: string
}>

/**
 * 商品发布筛选区：一个语义 form；收起态靠 Enter 提交，
 * 展开态由面板底部唯一主按钮「应用全部筛选」提交，两者走同一个 applyFilters。
 */
export function PublicationListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    appliedChips,
    removeFilter,
    clearAllFilters,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    applyFilters,
    resetMoreFilters,
    mallDraft,
    setMallDraft,
    publicationStatusDraft,
    setPublicationStatusDraft,
    deliveryStatusDraft,
    setDeliveryStatusDraft,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    appliedChips: readonly PublicationAppliedChip[]
    removeFilter: (key: PublicationFilterKey) => void
    clearAllFilters: () => void
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    applyFilters: () => void
    resetMoreFilters: () => void
    mallDraft: string | null
    setMallDraft: SetState<string | null>
    publicationStatusDraft: PublicationStatus | "all"
    setPublicationStatusDraft: SetState<PublicationStatus | "all">
    deliveryStatusDraft: PublicationDeliveryStatusSelection
    setDeliveryStatusDraft: SetState<PublicationDeliveryStatusSelection>
}) {
    const panelId = React.useId()
    const hasChips = appliedChips.length > 0

    return (
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
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="发布编号、SKU、商品名（/ 聚焦）"
                            aria-label="搜索发布编号、SKU 或商品名"
                        />
                    </InputGroup>
                }
                filters={
                    <Button
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
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
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
                                    aria-label="商品发布更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="发送状态"
                                        value={deliveryStatusDraft}
                                        onValueChange={setDeliveryStatusDraft}
                                        options={
                                            PUBLICATION_DELIVERY_STATUS_RADIO_FILTER_OPTIONS
                                        }
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                目标商城
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={mallDraft ?? undefined}
                                                onValueChange={setMallDraft}
                                                options={MALLS.map((mall) => ({
                                                    value: mall.id,
                                                    label: mall.name,
                                                }))}
                                                placeholder="全部商城"
                                                searchPlaceholder="搜索商城名称或代码"
                                                aria-label="目标商城"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                发布状态
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={
                                                    publicationStatusDraft ===
                                                    "all"
                                                        ? undefined
                                                        : publicationStatusDraft
                                                }
                                                onValueChange={(value) =>
                                                    setPublicationStatusDraft(
                                                        (value ??
                                                            "all") as PublicationStatus,
                                                    )
                                                }
                                                options={
                                                    PUBLICATION_STATUS_FILTER_OPTIONS
                                                }
                                                placeholder="有效发布"
                                                aria-label="发布状态"
                                            />
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button type="submit">
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
