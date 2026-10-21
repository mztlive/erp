import type { InquiryRequirement } from "../types"

export type DemandFormValues = {
    customer_id: string
    customer_name: string
    sales_owner_user_id: string
    business_org_unit_id: string
    deadline_at: number
    copied_from_inquiry_id?: string | null
    draft: InquiryRequirement
}

export const demandFormDefaults: DemandFormValues = {
    customer_id: "",
    customer_name: "",
    sales_owner_user_id: "",
    business_org_unit_id: "",
    deadline_at: 0,
    draft: { lines: [], note: "" },
}
