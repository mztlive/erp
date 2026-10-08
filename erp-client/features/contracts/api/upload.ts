import {
    apiGet,
    apiGetBlob,
    apiPost,
    apiPostForm,
    createApiError,
    type Page,
} from "@/lib/api"
import { contractPdfError } from "@/features/contracts/lib/pdf"
import type { UploadContractPdfInput } from "@/features/contracts/types"

export type ContractImportStage =
    | "reading_file"
    | "ocr"
    | "ai_extract"
    | "preparing_review"

export type ContractImportTask = {
    source_file_asset_id: string
    id: string
    version: number
    file_name: string
    page_count: number
    status: "ready" | "processing" | "failed" | "review" | "succeeded"
    stage?: ContractImportStage | null
    customer_id?: string
    expected_customer_id?: string
    started_at?: number
    recoverable_at?: number
    revision_target?: { contract_id: string; version: number }
    draft?: { fields: Record<string, string | null>; warnings: string[] } | null
    extraction?: {
        conflicts: string[]
        fields: Record<string, { value: string; page: number; quote: string }>
    }
    failure?: { code: string; message: string; field?: string; page?: number }
    result?: {
        id: string
        contract_no: string
        revision_id: string
        revision_no: number
        file_name: string
        created_at: number
    }
}

/** 文件与任务先持久化；业务字段一律不从客户端提交。 */
export async function uploadContractPdf(
    input: UploadContractPdfInput,
): Promise<ContractImportTask> {
    const error = contractPdfError(input.pdfFile)
    if (error)
        throw createApiError({
            kind: "Validation",
            message: error,
            status: 400,
            retryable: false,
        })
    const form = new FormData()
    form.append("file", input.pdfFile, input.pdfFile.name)
    form.append(
        "command",
        JSON.stringify({
            request_key: input.idempotencyKey,
            expected_customer_id: input.customerId || null,
            revision_target: input.revisionTarget
                ? {
                      contract_id: input.revisionTarget.contractId,
                      version: input.revisionTarget.version,
                  }
                : null,
        }),
    )
    return apiPostForm<ContractImportTask>("/admin/contracts/upload", form, {
        timeoutMs: 60_000,
    })
}

export const fetchContractImports = (
    page: number,
    revisionContractId?: string,
) =>
    apiGet<Page<ContractImportTask>>("/admin/contract-imports", {
        page,
        revision_contract_id: revisionContractId,
    })
export const fetchContractImport = (id: string) =>
    apiGet<ContractImportTask>(
        `/admin/contract-imports/${encodeURIComponent(id)}`,
    )
export const runContractImport = (id: string) =>
    apiPost<ContractImportTask>(
        `/admin/contract-imports/${encodeURIComponent(id)}/run`,
        undefined,
        { timeoutMs: 30_000 },
    )
export const previewContractImport = (id: string) =>
    apiGetBlob(`/admin/contract-imports/${encodeURIComponent(id)}/preview`)

export type ConfirmContractImport = {
    version: number
    fields: Record<string, string | null>
}
export const confirmContractImport = (input: {
    id: string
    command: ConfirmContractImport
}) =>
    apiPost<ContractImportTask>(
        `/admin/contract-imports/${encodeURIComponent(input.id)}/confirm`,
        input.command,
    )
