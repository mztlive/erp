import type { UseQueryResult } from "@tanstack/react-query"

import { getErrorMessage } from "@/lib/api/errors"
import { mergeSelected } from "@/features/entity-selectors/lib/merge-selected"

export type RemoteSearchListQuery<TItem> = Pick<
    UseQueryResult<readonly TItem[], Error>,
    "data" | "isFetching" | "isError" | "error"
>

export type RemoteSearchSelectedQuery<TItem> = Pick<
    UseQueryResult<TItem | null, Error>,
    "data" | "isFetching"
>

export type RemoteSearchComboboxOptions<TItem> = {
    list: RemoteSearchListQuery<TItem>
    selected?: RemoteSearchSelectedQuery<TItem>
    selectedItem?: TItem
    idOf: (item: TItem) => string
    emptyLabel?: string
    /** 列表查询失败时的兜底提示。 */
    fallbackError: string
    /** 额外加载态（如权限数据未就绪）。 */
    extraLoading?: boolean
}

/** 汇总远程搜索组合框的列表合并、加载态与空态文案。 */
export function useRemoteSearchCombobox<TItem>(
    options: RemoteSearchComboboxOptions<TItem>,
) {
    const selectedRow = options.selectedItem ?? options.selected?.data
    const rows = mergeSelected(options.list.data, selectedRow, options.idOf)
    return {
        rows,
        loading:
            options.extraLoading === true ||
            options.list.isFetching ||
            // 已选项详情在列表里已有时不必进入加载态，否则选中后输入框会闪「正在加载」。
            (Boolean(options.selected?.isFetching) && selectedRow == null),
        emptyLabel: options.list.isError
            ? getErrorMessage(options.list.error, options.fallbackError)
            : options.emptyLabel,
    }
}
