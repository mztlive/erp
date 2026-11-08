"use client"

import type { ComponentProps } from "react"

import { OptionCombobox } from "@/components/business"
import type { ComboboxOption } from "@/components/business/option-combobox"
import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { DirectoryFilterControl } from "@/features/master-data/components/list/directory-filter-control"
import {
    directoryOptionEmptyLabel,
    type ProductFilterDirectoryState,
} from "@/features/master-data/lib/product-filter-directory"

/**
 * 商品目录里的品牌、供应商筛选。
 * 无查询权限时整段不渲染，避免留下一个空的字段名。
 */
export function DirectoryOptionFilter({
    permission,
    unavailable,
    id,
    label,
    comboboxClassName,
    ariaLabel,
    value,
    onValueChange,
    options,
    loading,
    placeholder,
    searchPlaceholder,
    unavailableLabel,
    noScope,
    failed,
    emptyLabel,
    error,
    onRetry,
    onRefresh,
}: {
    permission: string
    unavailable: boolean
    id: string
    label: string
    comboboxClassName: string
    ariaLabel: string
    value?: string | null
    onValueChange: (value: string | null) => void
    options: readonly ComboboxOption[]
    loading: boolean
    placeholder: string
    searchPlaceholder: string
    unavailableLabel: string
    noScope: boolean
    failed: boolean
    emptyLabel: string
    error?: unknown
    onRetry: () => void
    onRefresh: () => Promise<unknown>
}) {
    if (unavailable) return null
    return (
        <PermissionGuard permission={permission}>
            <ListWorkspaceFilterField htmlFor={id} label={label}>
                <DirectoryFilterControl
                    id={id}
                    failed={failed}
                    unavailable={unavailable}
                    noScope={noScope}
                    error={error}
                    onRetry={onRetry}
                >
                    <OptionCombobox
                        onRefresh={onRefresh}
                        id={id}
                        className={comboboxClassName}
                        value={value}
                        aria-label={ariaLabel}
                        onValueChange={onValueChange}
                        options={options}
                        loading={loading}
                        retainMissingSelection={false}
                        placeholder={placeholder}
                        emptyLabel={directoryOptionEmptyLabel({
                            unavailable,
                            unavailableLabel,
                            noScope,
                            failed,
                            emptyLabel,
                        })}
                        searchPlaceholder={searchPlaceholder}
                    />
                </DirectoryFilterControl>
            </ListWorkspaceFilterField>
        </PermissionGuard>
    )
}

type DirectoryFilterBindings = Pick<
    ComponentProps<typeof DirectoryOptionFilter>,
    | "permission"
    | "unavailable"
    | "label"
    | "options"
    | "loading"
    | "placeholder"
    | "searchPlaceholder"
    | "unavailableLabel"
    | "noScope"
    | "failed"
    | "emptyLabel"
    | "error"
    | "onRetry"
    | "onRefresh"
>

/** 品牌筛选里各页面相同的权限、空态和刷新接线。id 与样式留在调用处。 */
export function brandDirectoryFilterBindings(
    directory: ProductFilterDirectoryState,
    onRefresh: () => Promise<unknown>,
): DirectoryFilterBindings {
    return {
        permission: "product_brand:list",
        unavailable: directory.brandUnavailable,
        label: "品牌",
        options: directory.brands,
        loading: directory.fetching,
        placeholder: "全部品牌",
        searchPlaceholder: "搜索品牌名称或代码",
        unavailableLabel: "当前账号无品牌查询权限",
        noScope: directory.brandNoScope,
        failed: directory.failed,
        emptyLabel: "没有符合条件的品牌",
        error: directory.error,
        onRetry: directory.refetch,
        onRefresh,
    }
}

/** 供应商筛选里各页面相同的权限、空态和刷新接线。id 与样式留在调用处。 */
export function supplierDirectoryFilterBindings(
    directory: ProductFilterDirectoryState,
    onRefresh: () => Promise<unknown>,
): DirectoryFilterBindings {
    return {
        permission: "supplier:list",
        unavailable: directory.supplierUnavailable,
        label: "供应商",
        options: directory.suppliers,
        loading: directory.fetching,
        placeholder: "全部供应商",
        searchPlaceholder: "搜索供应商名称或代码",
        unavailableLabel: "当前账号无供应商查询权限",
        noScope: directory.supplierNoScope,
        failed: directory.failed,
        emptyLabel: "没有符合条件的供应商",
        error: directory.error,
        onRetry: directory.refetch,
        onRefresh,
    }
}
