import { apiGet, apiGetBlob, apiPost, apiPostForm, type Page } from "@/lib/api"

export type NumberGroup = "FSY" | "ZHYF" | "GYL" | "BDKJ"
export const NUMBER_GROUP_OPTIONS = [
    { value: "FSY", label: "FSY · 福尚云（科技、食品、研发、科技深圳分公司）" },
    { value: "ZHYF", label: "ZHYF · 智慧云福" },
    { value: "GYL", label: "GYL · 朱太帅（沿用原供应链编号）" },
    { value: "BDKJ", label: "BDKJ · 炳达科技" },
] as const
export const DOCX_MIME =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"

export type ContractTemplate = {
    id: string
    version: number
    name: string
    company_id: string
    company_name: string
    group: NumberGroup
    file_name: string
    enabled: boolean
    created_at: number
}
export type ContractApplication = {
    id: string
    template_name: string
    company_name: string
    purpose: string
    contract_no: string
    created_at: number
}
export type ContractCounter = {
    group: NumberGroup
    year: number
    last_sequence: number
    version: number | null
}
export type ApplyContractInput = {
    command_id: string
    template_id: string
    purpose: string
}
export type TemplatePageParams = {
    page: number
    page_size: number
    include_disabled?: boolean
}

export const fetchContractTemplates = (params: TemplatePageParams) =>
    apiGet<Page<ContractTemplate>>("/admin/contract-templates", params)
export const fetchContractApplications = (params: TemplatePageParams) =>
    apiGet<Page<ContractApplication>>("/admin/contract-applications", params)
export const fetchContractCounters = () =>
    apiGet<ContractCounter[]>("/admin/contract-number-counters")
export const applyContractTemplate = (input: ApplyContractInput) =>
    apiPost<ContractApplication>("/admin/contract-applications", input)
export const setContractTemplateStatus = (input: {
    id: string
    version: number
    enabled: boolean
}) =>
    apiPost<ContractTemplate>(
        `/admin/contract-templates/${encodeURIComponent(input.id)}/status`,
        { version: input.version, enabled: input.enabled },
    )
export const configureContractCounter = (input: ContractCounter) =>
    apiPost<ContractCounter>("/admin/contract-number-counters", input)

export const uploadContractTemplate = (input: {
    name: string
    company_id: string
    group: NumberGroup
    file: File
}) => {
    const form = new FormData()
    form.append(
        "command",
        JSON.stringify({
            name: input.name,
            company_id: input.company_id,
            group: input.group,
        }),
    )
    form.append("file", input.file, input.file.name)
    return apiPostForm<ContractTemplate>("/admin/contract-templates", form, {
        timeoutMs: 60_000,
    })
}

export async function downloadContractWord(input: {
    id: string
    filename: string
    sample?: boolean
}) {
    const path = input.sample
        ? `/admin/contract-templates/${encodeURIComponent(input.id)}/sample`
        : `/admin/contract-applications/${encodeURIComponent(input.id)}/download`
    const blob = await apiGetBlob(path, { timeoutMs: 60_000 })
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.href = url
    link.download = input.filename
    link.click()
    // 浏览器下载读取完成后释放；下载链接不依赖临时公开文件地址。
    window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
}
