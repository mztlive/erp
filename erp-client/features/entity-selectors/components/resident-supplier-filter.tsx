"use client"

import { OptionCombobox } from "@/components/business/option-combobox"
import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useSupplierSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

/**
 * 列表筛选栏常驻的供应商筛选。
 *
 * 与 `SupplierSearchCombobox` 共用候选查询，外观固定为筛选栏的 `OptionCombobox`。
 */
export function ResidentSupplierFilter({
    id,
    value,
    onValueChange,
}: {
    id: string
    value: string | null
    onValueChange: (value: string | null) => void
}) {
    const profile = useAccountProfileQuery()
    const canList = hasPermission(profile.data?.permissions, "supplier:list")
    const search = useSearchInput()
    const query = useSupplierSelectorQuery(
        { query: search.input, purpose: "filter" },
        value ?? undefined,
        canList,
    )
    const { rows, loading, emptyLabel, onRefresh, denied } =
        useRemoteSearchCombobox({
            selectedId: value ?? undefined,
            list: query.list,
            selected: query.selected,
            idOf: (item) => item.supplierId,
            fallbackError: "供应商加载失败，请重试",
        })
    if (!canList || denied) return null

    const directoryFailed = query.list.isError || query.selected.isError
    return (
        <div className="min-w-0">
            <OptionCombobox
                id={id}
                className="w-56 max-w-full min-w-0"
                filterLabel="供应商"
                aria-label="供应商"
                placeholder="全部"
                searchPlaceholder="搜索供应商名称或编码"
                filterMode="remote"
                onSearchChange={search.onSearchChange}
                loading={loading}
                onRefresh={onRefresh}
                emptyLabel={emptyLabel}
                value={value}
                onValueChange={onValueChange}
                options={rows.map((item) => ({
                    value: item.supplierId,
                    label: item.supplierName,
                    keywords: item.supplierCode,
                }))}
            />
            <SelectorQueryFeedback
                id={id}
                failed={directoryFailed}
                error={query.list.error ?? query.selected.error}
                noScope={
                    !query.list.isFetching &&
                    !directoryFailed &&
                    query.list.emptyReason === "no_scope"
                }
                onRetry={() => {
                    void query.list.refetch()
                    if (value) void query.selected.refetch()
                }}
            />
        </div>
    )
}
