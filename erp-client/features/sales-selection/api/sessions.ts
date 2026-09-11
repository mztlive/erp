/**
 * 选品会话内部只读接口：销售查看客户已保存选择。
 * 写会话与提交只走公开链路（见 api/public.ts）。
 */

import { apiGet } from "@/lib/api"
import type { PublicChoiceView } from "@/features/sales-selection/types"

/** 内部会话快照。 */
export type BookSessionView = Readonly<{
    book_id: string
    expected_version: number
    selections: readonly PublicChoiceView[]
    total_amount?: string | null
    updated_at: number
}>

/**
 * 读取选品册当前会话（内部查看用）。
 * @param bookId 选品册身份
 */
export const fetchBookSession = async (
    bookId: string,
): Promise<BookSessionView> =>
    apiGet<BookSessionView>(`/admin/sales-selection-books/${bookId}/session`)
