"use client"

import {
    OptionCombobox,
    type OptionComboboxProps,
} from "@/components/business/option-combobox"
import { useMallSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"

export type MallSearchComboboxProps = Omit<
    OptionComboboxProps,
    "options" | "loading" | "filterMode" | "onSearchChange"
> & { purpose?: "filter" | "form" }

/** 商城来源系统选项；组件自包含取数并共享 TanStack Query 缓存。 */
export function MallSearchCombobox({
    purpose = "filter",
    emptyLabel,
    ...props
}: MallSearchComboboxProps) {
    const query = useMallSelectorQuery(purpose)
    return (
        <OptionCombobox
            {...props}
            options={(query.data ?? []).map((item) => ({
                value: item.id,
                label: item.name,
                keywords: item.code,
            }))}
            loading={query.isFetching}
            emptyLabel={
                query.isError
                    ? getErrorMessage(query.error, "商城加载失败，请重试")
                    : emptyLabel
            }
        />
    )
}
