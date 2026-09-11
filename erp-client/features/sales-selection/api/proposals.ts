/**
 * 销售方案内部接口：只读查看客户提交结果。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { SelectionProposal } from "@/features/sales-selection/types"

/**
 * 读取销售方案详情（编号/客户/册/批次/形态/提交方式/版本/时间/来源+双层明细）。
 * @param proposalId 方案身份
 */
export const fetchProposalDetail = async (
    proposalId: string,
): Promise<SelectionProposal> =>
    apiGet<SelectionProposal>(`/admin/sales-selection-proposals/${proposalId}`)

/**
 * 按选品册查询其唯一方案（已提交行链方案详情用）。
 * 后端仅提供按 booklet_id 筛选的方案列表，取首条作为该册唯一方案。
 * @param bookId 选品册身份
 */
export const fetchProposalByBook = async (
    bookId: string,
): Promise<SelectionProposal> => {
    const page = await apiGet<Page<SelectionProposal>>(
        "/admin/sales-selection-proposals",
        { booklet_id: bookId, page: 1, page_size: 1 },
    )
    const first = page.items[0]
    if (!first) throw new Error("该选品册暂无已提交方案")
    return first
}

/** 方案列表查询。 */
export type ProposalListQuery = Readonly<{
    q?: string
    page?: number
    page_size?: number
}>

/**
 * 查询销售方案列表（内部核对用）。
 */
export const fetchProposals = async (
    query: ProposalListQuery,
): Promise<{ rows: SelectionProposal[]; total: number }> => {
    const page = await apiGet<Page<SelectionProposal>>(
        "/admin/sales-selection-proposals",
        {
            q: query.q || undefined,
            page: query.page ?? 1,
            page_size: query.page_size ?? 20,
        },
    )
    return { rows: page.items, total: page.total }
}
