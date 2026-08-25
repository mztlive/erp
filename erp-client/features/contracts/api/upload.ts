import {
    createApiError,
    getApiBaseUrl,
    getToken,
    notifyUnauthorized,
} from "@/lib/api"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { contractPdfError } from "@/features/contracts/lib/pdf"

import {
    paymentTermCodeFromLabel,
    tsToIso,
} from "@/features/contracts/api/helpers"
import type {
    UploadContractPdfInput,
    UploadContractPdfResult,
} from "@/features/contracts/types"

type BackendContractUpload = {
    id: string
    contract_no: string
    revision_id: string
    revision_no: number
    file_asset_id: string
    file_name: string
    created_at: number
}

/** 上传合同 PDF；前端只发一个 multipart 命令。 */
export async function uploadContractPdf(
    input: UploadContractPdfInput,
): Promise<UploadContractPdfResult> {
    const fileError = contractPdfError(input.pdfFile)
    if (fileError) {
        throw createApiError({
            kind: "Validation",
            message: fileError,
            status: 400,
            retryable: false,
        })
    }

    if (!input.customerId?.trim()) {
        throw createApiError({
            kind: "Validation",
            message: "请选择客户",
            status: 400,
            retryable: false,
        })
    }

    const termCode = paymentTermCodeFromLabel(input.paymentTerms)
    const termName =
        PAYMENT_TERM_OPTIONS.find((o) => o.value === termCode)?.label ??
        input.paymentTerms

    const command = {
        contract_no: input.contractNo.trim(),
        customer_id: input.customerId.trim(),
        settlement_party_id: input.settlementPartyId?.trim() || null,
        customer_name: input.customerName.trim(),
        settlement_party_name: input.settlementPartyName.trim(),
        payment_term_code: termCode,
        payment_term_name: termName,
        // UI 未采集开票快照：用受控默认值满足后端校验（见证据 gap）
        invoice_type: "增值税专用发票",
        tax_point: "13",
        valid_from: input.validFrom,
        valid_to: input.validTo || undefined,
        signed_at: input.signedAt,
    }
    const form = new FormData()
    // 服务端流式解析先取文件，再读取 JSON 命令；顺序是协议的一部分。
    form.append("file", input.pdfFile, input.pdfFile.name)
    form.append("command", JSON.stringify(command))
    const headers: Record<string, string> = {}
    const token = getToken()
    if (token) headers.Authorization = `Bearer ${token}`
    let response: Response
    try {
        response = await fetch(`${getApiBaseUrl()}/admin/contracts/upload`, {
            method: "POST",
            headers,
            body: form,
            signal: AbortSignal.timeout(60_000),
        })
    } catch (cause) {
        throw createApiError({
            kind: "Network",
            message: "网络请求失败或连接超时",
            cause,
        })
    }
    const text = await response.text()
    let parsed: unknown
    try {
        parsed = text ? JSON.parse(text) : null
    } catch (cause) {
        throw createApiError({
            kind: "Parse",
            message: "响应数据解析失败",
            cause,
            responseData: text,
        })
    }
    const envelope = parsed as {
        success?: boolean
        status?: number
        errorMessage?: string
        data?: BackendContractUpload | null
    } | null
    if (response.status === 401 || envelope?.status === 401) {
        notifyUnauthorized()
        throw createApiError({
            kind: "Auth",
            message: "登录状态已失效，请重新登录",
            status: 401,
            responseData: parsed,
        })
    }
    if (!response.ok || envelope?.success === false) {
        throw createApiError({
            kind: response.status === 400 ? "Validation" : "Http",
            message:
                envelope?.errorMessage ||
                (response.status === 400
                    ? "请求未通过业务校验"
                    : `请求失败（HTTP ${response.status}）`),
            status: response.status,
            responseData: parsed,
        })
    }
    const created = envelope?.data
    if (!created?.id || !created.revision_id) {
        throw createApiError({
            kind: "Parse",
            message: "上传响应缺少合同或修订身份",
            responseData: parsed,
        })
    }

    return {
        contractId: created.id,
        contractNo: created.contract_no,
        revisionId: created.revision_id,
        revisionNo: created.revision_no,
        uploadedAt: tsToIso(created.created_at),
        fileName: created.file_name,
        reference: `CT-UP-${created.contract_no}`,
    }
}
