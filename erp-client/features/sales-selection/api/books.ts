/**
 * 选品册内部接口：列表、创建、详情与册级操作。
 * 后端未就绪时按约定 DTO 联调；调用点必须经 TanStack Query。
 */

import { apiDelete, apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import {
    ACTIONABLE_BOOK_STATUSES,
    sumActionableBookCounts,
} from "@/features/sales-selection/lib/actionable-books"
import type {
    BookListQuery,
    CreateBookInput,
    PoolFilterSnapshot,
    SelectionBook,
    SelectionBookDetail,
} from "@/features/sales-selection/types"

/** 列表行 wire 形态（后端 snake_case 直接透传）。 */
type BookListWire = SelectionBook

/**
 * 查询选品册列表。
 * @param query 客户/形态/状态/关键字与分页
 */
export const fetchBooks = async (
    query: BookListQuery,
): Promise<{ rows: SelectionBook[]; total: number }> => {
    const page = await apiGet<Page<BookListWire>>(
        "/admin/sales-selection-books",
        {
            q: query.q || undefined,
            customer_id: query.customer_id || undefined,
            selection_form:
                query.selection_form && query.selection_form !== "ALL"
                    ? query.selection_form
                    : undefined,
            submit_mode:
                query.submit_mode && query.submit_mode !== "ALL"
                    ? query.submit_mode
                    : undefined,
            status:
                query.status && query.status !== "ALL"
                    ? query.status
                    : undefined,
            page: query.page ?? 1,
            page_size: query.page_size ?? 20,
        },
    )
    return { rows: page.items, total: page.total }
}

/**
 * 查询仍需销售处理的选品册数量（草稿 + 准备中 + 待发布）。
 * 按状态各取 total，不把列表行拉到侧栏。
 */
export const fetchActionableBookCount = async (): Promise<number> => {
    const pages = await Promise.all(
        ACTIONABLE_BOOK_STATUSES.map((status) =>
            fetchBooks({ status, page: 1, page_size: 1 }),
        ),
    )
    return sumActionableBookCounts(pages.map((page) => page.total))
}

/**
 * 读取选品册详情（含档位与陈列）。
 * @param bookId 选品册身份
 */
export const fetchBookDetail = async (
    bookId: string,
): Promise<SelectionBookDetail> =>
    apiGet<SelectionBookDetail>(`/admin/sales-selection-books/${bookId}`)

/**
 * 创建选品册：绑定客户、形态、提交方式与商品池来源。
 * @param input 创建载荷（含幂等键）
 */
export const createBook = async (
    input: CreateBookInput,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>("/admin/sales-selection-books", {
        customer_id: input.customer_id,
        form: input.selection_form,
        submit_mode: input.submit_mode,
        pool_source_kind: input.source_kind,
        pool_filter: input.filter ?? undefined,
        sku_ids: input.sku_ids ? [...input.sku_ids] : undefined,
        tiers: input.tiers
            ? input.tiers.map((tier) => ({ ...tier }))
            : undefined,
        idempotency_key: input.idempotency_key,
    })

/** 准备任务种类。 */
export type PrepareKind =
    | "FIRST_PREPARE"
    | "REGENERATED_TIERS"
    | "REGENERATED_ALL"
    | "RE_PREPARE"

/**
 * 启动准备任务（首次准备/按档重生成/整册重生成/整册重新准备）。
 * @param bookId 选品册身份
 * @param payload 任务种类、档位、筛选与幂等键
 */
export const startPrepareTask = async (
    bookId: string,
    payload: Readonly<{
        kind: PrepareKind
        tier_ids?: readonly string[]
        filter?: PoolFilterSnapshot
        sku_ids?: readonly string[]
        tiers?: CreateBookInput["tiers"]
        expected_version: number
        idempotency_key: string
    }>,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/prepare`,
        {
            kind: payload.kind,
            tier_ids: payload.tier_ids ? [...payload.tier_ids] : undefined,
            pool_filter: payload.filter ?? undefined,
            sku_ids: payload.sku_ids ? [...payload.sku_ids] : undefined,
            tiers: payload.tiers
                ? payload.tiers.map((tier) => ({ ...tier }))
                : undefined,
            expected_version: payload.expected_version,
            idempotency_key: payload.idempotency_key,
        },
    )

/**
 * 删除单个陈列项（待发布态）。
 * @param bookId 选品册身份
 * @param itemId 陈列项身份
 * @param expectedVersion 选品册版本
 */
export const deleteDisplayItem = async (
    bookId: string,
    itemId: string,
    expectedVersion: number,
): Promise<SelectionBookDetail> =>
    apiDelete<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/display-items/${itemId}?expected_version=${expectedVersion}`,
    )

/**
 * 发布选品册并生成公开链接。
 */
export const publishBook = async (
    bookId: string,
    payload: Readonly<{
        batch_id?: string
        expected_version: number
        idempotency_key: string
    }>,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/publish`,
        payload,
    )

/**
 * 更换公开链接（原令牌立即失效，会话与快照沿用）。
 */
export const replaceBookLink = async (
    bookId: string,
    payload: Readonly<{ expected_version: number; idempotency_key: string }>,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/replace-link`,
        payload,
    )

/**
 * 复制当前有效链接（返回可直接发给客户的地址）。
 */
export const copyBookLink = async (
    bookId: string,
): Promise<{ public_url: string }> =>
    apiGet<{ public_url: string }>(
        `/admin/sales-selection-books/${bookId}/link`,
    )

/**
 * 关闭未提交选品册。
 */
export const closeBook = async (
    bookId: string,
    payload: Readonly<{ expected_version: number; idempotency_key: string }>,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/close`,
        payload,
    )

/**
 * 撤销已提交选品册的链接访问（不改变已提交状态）。
 */
export const revokeBookLink = async (
    bookId: string,
    payload: Readonly<{ expected_version: number; idempotency_key: string }>,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/revoke-link`,
        payload,
    )

/**
 * 作废发布前选品册。
 */
export const voidBook = async (
    bookId: string,
    payload: Readonly<{ expected_version: number; idempotency_key: string }>,
): Promise<SelectionBookDetail> =>
    apiPost<SelectionBookDetail>(
        `/admin/sales-selection-books/${bookId}/void`,
        payload,
    )
