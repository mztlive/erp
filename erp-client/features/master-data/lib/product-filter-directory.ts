import {
    MISSING_SELECTION_CHECKING_LABEL,
    MISSING_SELECTION_UNAVAILABLE_LABEL,
} from "@/components/business/combobox-input-search"
import type { ProductFilterOptions } from "@/features/master-data/types"

/** 与 SelectorQueryFeedback 的无范围文案一致。 */
export const DIRECTORY_NO_SCOPE_MESSAGE =
    "当前角色无此目录的数据范围，请申请权限"

const DIRECTORY_LOAD_FAILED_MESSAGE = "候选加载失败，请重试"

/** 商品筛选目录查询的最小视图。完整 Query 结果可直接传入。 */
export type ProductFilterDirectoryQuery = {
    data?: ProductFilterOptions
    isPending: boolean
    isFetching?: boolean
    isError?: boolean
    isSuccess?: boolean
    error?: unknown
    refetch?: () => unknown
}

export type ProductFilterDirectoryState = {
    fetching: boolean
    failed: boolean
    error: unknown
    refetch: () => void
    categories: ProductFilterOptions["categories"]
    brands: ProductFilterOptions["brands"]
    suppliers: ProductFilterOptions["suppliers"]
    categoryUnavailable: boolean
    brandUnavailable: boolean
    supplierUnavailable: boolean
    categoryNoScope: boolean
    brandNoScope: boolean
    supplierNoScope: boolean
}

/**
 * 请求中或失败时不交出上一份目录。
 * 只有本次成功且不在请求中，才使用返回的名称和 empty_reason。
 */
export function productFilterDirectoryState(
    query: ProductFilterDirectoryQuery,
): ProductFilterDirectoryState {
    const fetching = query.isPending || query.isFetching === true
    const failed = query.isError === true
    const data =
        query.isSuccess === true && !fetching && !failed
            ? query.data
            : undefined
    const unavailable = data?.unavailable
    return {
        fetching,
        failed,
        error: query.error,
        refetch: () => {
            void query.refetch?.()
        },
        categories: data?.categories ?? [],
        brands: data?.brands ?? [],
        suppliers: data?.suppliers ?? [],
        categoryUnavailable: unavailable?.includes("categories") ?? false,
        brandUnavailable: unavailable?.includes("brands") ?? false,
        supplierUnavailable: unavailable?.includes("suppliers") ?? false,
        categoryNoScope: data?.emptyReasons?.categories === "no_scope",
        brandNoScope: data?.emptyReasons?.brands === "no_scope",
        supplierNoScope: data?.emptyReasons?.suppliers === "no_scope",
    }
}

/** 已应用标签与控件共用：请求中不显示旧名称，失败或本次没有该对象时不显示内部 ID。 */
export function directorySelectionLabel<T>(
    id: string | null | undefined,
    options: readonly T[],
    idOf: (item: T) => string,
    labelOf: (item: T) => string,
    state: Pick<ProductFilterDirectoryState, "fetching" | "failed">,
): string | undefined {
    if (!id) return undefined
    if (state.fetching) return MISSING_SELECTION_CHECKING_LABEL
    if (state.failed) return MISSING_SELECTION_UNAVAILABLE_LABEL
    const found = options.find((item) => idOf(item) === id)
    return found ? labelOf(found) : MISSING_SELECTION_UNAVAILABLE_LABEL
}

/** 缺动作权限沿用原拒绝文案；no_scope 与网络失败分开，不互相伪装。 */
export function directoryOptionEmptyLabel(input: {
    unavailable: boolean
    unavailableLabel: string
    noScope: boolean
    failed: boolean
    emptyLabel: string
}): string {
    if (input.unavailable) return input.unavailableLabel
    if (input.failed) return DIRECTORY_LOAD_FAILED_MESSAGE
    if (input.noScope) return DIRECTORY_NO_SCOPE_MESSAGE
    return input.emptyLabel
}
