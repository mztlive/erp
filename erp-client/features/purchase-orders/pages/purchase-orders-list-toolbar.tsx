"use client"

import type { RefObject } from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import type { usePurchaseOrdersListFilters } from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import {
    PURCHASE_TYPE_LABEL,
    type PurchaseType,
} from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { listFilterText } from "@/lib/ui-text"

const PREFIX = "procurement-orders-list"
const PURCHASE_TYPE_OPTIONS = [
    { value: "all", label: "全部" },
    ...Object.entries(PURCHASE_TYPE_LABEL).map(([value, label]) => ({
        value,
        label,
    })),
]

export function PurchaseOrdersListToolbar({
    filters,
    searchInputRef,
    sourceOrderNo,
    loading,
    failed,
}: {
    filters: Pick<
        ReturnType<typeof usePurchaseOrdersListFilters>,
        | "typeDraft"
        | "setTypeDraft"
        | "canFilterOwner"
        | "ownerDraft"
        | "setOwnerDraft"
        | "searchDraft"
        | "setSearchDraft"
        | "salesOrderFromUrl"
        | "clearSourceOrder"
        | "applyFilters"
        | "clearAllFilters"
        | "hasActiveFilters"
        | "hasPendingChanges"
    >
    searchInputRef: RefObject<HTMLInputElement | null>
    sourceOrderNo?: string
    loading: boolean
    failed: boolean
}) {
    const sourceLabel = sourceOrderNo
        ? `来源销售单：${sourceOrderNo}`
        : "来源销售单：指定单据"

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            idPrefix={`${PREFIX}-filter`}
            formAriaLabel="采购单查询"
            onSubmit={filters.applyFilters}
            search={
                <ListSearchField
                    id={`${PREFIX}-search`}
                    searchInputRef={searchInputRef}
                    data-slot="po-list-search"
                    value={filters.searchDraft}
                    onChange={filters.setSearchDraft}
                    placeholder="采购单号、供应商、来源销售单"
                    aria-label="搜索采购单"
                />
            }
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${PREFIX}-type`}
                        getOptionId={(value) =>
                            `${PREFIX}-type-${toAutomationIdSegment(value)}`
                        }
                        aria-label="采购类型"
                        className="w-36 max-w-full min-w-0"
                        filterLabel="采购类型"
                        value={
                            filters.typeDraft === "all"
                                ? null
                                : filters.typeDraft
                        }
                        options={PURCHASE_TYPE_OPTIONS}
                        placeholder="全部"
                        onValueChange={(value) =>
                            filters.setTypeDraft(
                                (value ?? "all") as PurchaseType | "all",
                            )
                        }
                    />
                    {filters.canFilterOwner ? (
                        <div className="w-56 max-w-full min-w-0 empty:hidden">
                            <PersonDirectoryFilter
                                id={`${PREFIX}-owner`}
                                category="procurement"
                                label="采购负责人"
                                hideLabel
                                value={filters.ownerDraft}
                                onChange={filters.setOwnerDraft}
                            />
                        </div>
                    ) : null}
                </>
            }
            queryButtonId={`${PREFIX}-apply-filters`}
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            foldedChipKeys={["salesOrderId"]}
            chips={
                filters.salesOrderFromUrl
                    ? [
                          {
                              key: "salesOrderId",
                              label: sourceLabel,
                              onClear: filters.clearSourceOrder,
                          },
                      ]
                    : undefined
            }
            onClearAll={filters.clearAllFilters}
            clearButtonId={`${PREFIX}-clear-all`}
            extraPrimary={
                filters.hasActiveFilters && !filters.salesOrderFromUrl ? (
                    <Button
                        id={`${PREFIX}-clear-all`}
                        type="button"
                        variant="ghost"
                        className="font-normal text-muted-foreground"
                        onClick={filters.clearAllFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : null
            }
            hasPendingChanges={filters.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
