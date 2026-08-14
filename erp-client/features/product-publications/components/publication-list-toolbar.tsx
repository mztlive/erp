"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { FilterChip } from "@/components/business/filter-chip"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { MALLS } from "@/features/product-publications/api/publications"
import { PUBLICATION_STATUS_LABEL } from "@/features/product-publications/types"

export function PublicationListToolbar({
    searchInput,
    searchInputRef,
    onSearchInputChange,
    onSearchCommit,
    mallId,
    publicationStatus,
    deliveryStatus,
    skuId,
    supplierOfferingRevisionId,
    resolvedSkuCode,
    resolvedSupplierName,
    filterSummary,
    hasActiveFilters,
    onPatch,
    onClearFilters,
}: {
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchInputChange: (value: string) => void
    onSearchCommit: () => void
    mallId?: string
    publicationStatus: string
    deliveryStatus: string
    skuId?: string
    supplierOfferingRevisionId?: string
    resolvedSkuCode?: string
    resolvedSupplierName?: string
    filterSummary?: string
    hasActiveFilters: boolean
    /** 以当前 URL 快照合并补丁并写回 URL（见 usePublicationListFilters） */
    onPatch: (patch: Record<string, string | undefined>) => void
    onClearFilters: () => void
}) {
    return (
        <ListToolbar
            search={
                <InputGroup className="max-w-md">
                    <InputGroupAddon>
                        <SearchIcon className="size-4" />
                    </InputGroupAddon>
                    <InputGroupInput
                        ref={searchInputRef}
                        value={searchInput}
                        placeholder="发布编号、SKU、商品名（/ 聚焦）"
                        onChange={(e) => onSearchInputChange(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") onSearchCommit()
                        }}
                    />
                </InputGroup>
            }
            filters={
                <>
                    <OptionCombobox
                        value={mallId ?? "all"}
                        onValueChange={(v) => {
                            const next = v ?? "all"
                            onPatch({
                                mall: next === "all" ? undefined : next,
                            })
                        }}
                        options={[
                            { value: "all", label: "全部商城" },
                            ...MALLS.map((m) => ({
                                value: m.id,
                                label: m.name,
                            })),
                        ]}
                        className="w-36"
                        size="sm"
                        allowClear={false}
                        aria-label="目标商城"
                        placeholder="全部商城"
                    />
                    <OptionCombobox
                        value={publicationStatus}
                        onValueChange={(v) =>
                            onPatch({
                                publicationStatus: v ?? "all",
                                metric: undefined,
                            })
                        }
                        options={[
                            { value: "all", label: "有效发布" },
                            ...(
                                Object.keys(
                                    PUBLICATION_STATUS_LABEL,
                                ) as Array<keyof typeof PUBLICATION_STATUS_LABEL>
                            ).map((k) => ({
                                value: k,
                                label: PUBLICATION_STATUS_LABEL[k],
                            })),
                        ]}
                        className="w-36"
                        size="sm"
                        allowClear={false}
                        aria-label="发布状态"
                        placeholder="发布状态"
                    />
                    <OptionCombobox
                        value={deliveryStatus}
                        onValueChange={(v) =>
                            onPatch({
                                deliveryStatus: v ?? "all",
                                metric: undefined,
                            })
                        }
                        options={[
                            { value: "all", label: "发送状态" },
                            {
                                value: "pending_confirm",
                                label: "待商城确认",
                            },
                            { value: "failed", label: "失败" },
                            { value: "handoff", label: "转人工" },
                            { value: "acked", label: "已确认" },
                        ]}
                        className="w-40"
                        size="sm"
                        allowClear={false}
                        aria-label="发送状态"
                        placeholder="发送状态"
                    />
                </>
            }
            secondary={
                (skuId && resolvedSkuCode) ||
                (supplierOfferingRevisionId && resolvedSupplierName) ||
                filterSummary ? (
                    <>
                        {skuId && resolvedSkuCode ? (
                            <FilterChip
                                label={`已按 SKU：${resolvedSkuCode}`}
                                clearLabel={`移除按 ${resolvedSkuCode} 筛选`}
                                onClear={() =>
                                    onPatch({
                                        skuId: undefined,
                                        supplierOfferingRevisionId: undefined,
                                    })
                                }
                            />
                        ) : null}
                        {supplierOfferingRevisionId &&
                        resolvedSupplierName ? (
                            <FilterChip
                                label={`已按固定供给：${resolvedSupplierName}`}
                                clearLabel={`移除按 ${resolvedSupplierName} 筛选`}
                                onClear={() =>
                                    onPatch({
                                        skuId: undefined,
                                        supplierOfferingRevisionId: undefined,
                                    })
                                }
                            />
                        ) : null}
                        {filterSummary ? (
                            <span className="text-xs text-muted-foreground">
                                {filterSummary}
                            </span>
                        ) : null}
                    </>
                ) : undefined
            }
            actions={
                <>
                    {hasActiveFilters && (
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    )}
                </>
            }
        />
    )
}
