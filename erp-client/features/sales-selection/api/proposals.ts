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

/** 方案列表查询。 */
export type ProposalListQuery = Readonly<{
    q?: string
    page?: number
    page_size?: number
    scope_version?: string
}>
