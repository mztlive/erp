"use client"

import { OptionCombobox } from "@/components/business"
import type { ComboboxOption } from "@/components/business/option-combobox"
import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { DirectoryFilterControl } from "@/features/master-data/components/list/directory-filter-control"
import { directoryOptionEmptyLabel } from "@/features/master-data/lib/product-filter-directory"

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
