/**
 * 销售方案内部接口：只读查看客户提交结果。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { SelectionProposal } from "@/features/sales-selection/types"

/** 方案列表行。显示名来自已授权业务行，不是筛选候选。 */
export type ProposalListItem = {
    id: string
    proposal_no: string
    customer_name: string
    booklet_id: string
    sales_owner_user_id: string
    sales_owner_name?: string | null
    business_org_unit_id: string
    form: SelectionProposal["form"]
    submit_mode: SelectionProposal["submit_mode"]
    submitted_at: number
}

/** 方案列表视图：分页与范围版本，不含负责人候选。 */
type ProposalListView = {
    page: Page<ProposalListItem>
    scope_version: string
    policy_version: number
    organization_version: number
    no_scope: boolean
}

/**
 * 读取销售方案详情（编号/客户/册/批次/形态/提交方式/版本/时间/来源+双层明细）。
 * @param proposalId 方案身份
 */
export const fetchProposalDetail = async (
    proposalId: string,
): Promise<SelectionProposal> =>
    apiGet<SelectionProposal>(`/admin/sales-selection-proposals/${proposalId}`)

/**
 * 按选品册读取方案列表首行。
 * 列表不含明细；打开方案用 fetchProposalDetail。
 * @param bookId 选品册身份
 */
export const fetchProposalByBook = async (
    bookId: string,
): Promise<ProposalListItem> => {
    const view = await apiGet<ProposalListView>(
        "/admin/sales-selection-proposals",
        { booklet_id: bookId, page: 1, page_size: 1 },
    )
    const first = view.page.items[0]
    if (!first) throw new Error("该选品册暂无已提交方案")
    return first
}

/** 方案列表查询。 */
export type ProposalListQuery = Readonly<{
    q?: string
    page?: number
    page_size?: number
    scope_version?: string
}>

/**
 * 查询销售方案列表（内部核对用）。
 */
export const fetchProposals = async (
    query: ProposalListQuery,
): Promise<{
    rows: ProposalListItem[]
    total: number
    scope_version: string
    policy_version: number
    organization_version: number
    no_scope: boolean
}> => {
    const view = await apiGet<ProposalListView>(
        "/admin/sales-selection-proposals",
        {
            q: query.q || undefined,
            page: query.page ?? 1,
            page_size: query.page_size ?? 20,
            scope_version: query.scope_version,
        },
    )
    return {
        rows: view.page.items,
        total: view.page.total,
        scope_version: view.scope_version,
        policy_version: view.policy_version,
        organization_version: view.organization_version,
        no_scope: view.no_scope,
    }
}
