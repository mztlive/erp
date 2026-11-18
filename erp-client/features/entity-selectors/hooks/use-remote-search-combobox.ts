import type { UseQueryResult, RefetchOptions } from "@tanstack/react-query"

import { getErrorMessage, isPermissionDeniedError } from "@/lib/api/errors"
import { mergeSelected } from "@/features/entity-selectors/lib/merge-selected"

export type RemoteSearchListQuery<TItem> = Pick<
    UseQueryResult<readonly TItem[], Error>,
    "data" | "isFetching" | "isError" | "error"
> & {
    emptyReason?: string | null
    refetch: (options?: RefetchOptions) => Promise<unknown>
}

export type RemoteSearchSelectedQuery<TItem> = Pick<
    UseQueryResult<TItem | null, Error>,
    "data" | "isFetching" | "isError" | "error"
> & { refetch: (options?: RefetchOptions) => Promise<unknown> }

export type RemoteSearchComboboxOptions<TItem> = {
    list: RemoteSearchListQuery<TItem>
    selected?: RemoteSearchSelectedQuery<TItem>
    selectedItem?: TItem
    selectedId?: string
    idOf: (item: TItem) => string
    emptyLabel?: string
    noScopeLabel?: string
    /** 列表查询失败时的兜底提示。 */
    fallbackError: string
    /** 额外加载态（如权限数据未就绪）。 */
    extraLoading?: boolean
    /** 权限尚未确认或确认失败时撤下全部名称。 */
    blocked?: boolean
}

/** 汇总远程搜索组合框的列表合并、加载态与空态文案。 */
export function useRemoteSearchCombobox<TItem>(
    options: RemoteSearchComboboxOptions<TItem>,
) {
    const denied = isPermissionDeniedError(options.list.error)
    const selectedError = isPermissionDeniedError(options.selected?.error)
        ? undefined
        : options.selected?.error
    const blocked = denied || options.blocked || options.extraLoading
    const selectedRow = options.selected
        ? blocked || options.selected.isError || options.selected.isFetching
            ? undefined
            : options.selected.data
        : blocked || options.list.isError || options.list.isFetching
          ? undefined
          : options.selectedItem
    const rows = mergeSelected(
        blocked || options.list.isError || options.list.isFetching
            ? []
            : options.list.data?.filter(
                  (item) =>
                      !options.selected ||
                      options.idOf(item) !== options.selectedId,
              ),
        selectedRow,
        options.idOf,
    )
    return {
        rows,
        denied,
        onRefresh: blocked
            ? undefined
            : () =>
                  Promise.all([
                      options.list.refetch({ throwOnError: true }),
                      ...(options.selected &&
                      options.selectedId &&
                      !isPermissionDeniedError(options.selected.error)
                          ? [options.selected.refetch({ throwOnError: true })]
                          : []),
                  ]),
        loading:
            options.extraLoading === true ||
            options.list.isFetching ||
            // 回显正在核对时显示加载态；不得由列表旧项恢复名称。
            (Boolean(options.selected?.isFetching) && selectedRow == null),
        emptyLabel:
            options.list.isError || selectedError
                ? getErrorMessage(
                      options.list.error ?? selectedError,
                      options.fallbackError,
                  )
                : options.list.emptyReason === "no_scope"
                  ? (options.noScopeLabel ??
                    "暂无该资料的查看权限，请联系管理员开通")
                  : options.emptyLabel,
    }
}
