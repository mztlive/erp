import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"

export type Company = {
    id: string
    party_no: string
    version: number
    legal_name: string
    short_name: string | null
    aliases: string[]
    unified_credit_code: string | null
    status: "active" | "disabled"
}
export type CompanyInput = Omit<Company, "id" | "version"> & {
    version?: number
}
export const fetchCompanies = (params: {
    keyword?: string
    status?: string
    page?: number
    page_size?: number
}) => apiGet<Page<Company>>("/admin/companies", params)
export const fetchCompany = (id: string) =>
    apiGet<Company>(`/admin/companies/${encodeURIComponent(id)}`)
export const saveCompany = ({
    id,
    input,
}: {
    id?: string
    input: CompanyInput
}) =>
    id
        ? apiPut<Company>(`/admin/companies/${encodeURIComponent(id)}`, input)
        : apiPost<Company>("/admin/companies", input)
