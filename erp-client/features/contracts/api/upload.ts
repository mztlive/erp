import { createApiError, apiPostForm } from "@/lib/api"
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

    // 统一信封解包：网络 / 鉴权 / 业务失败由 lib/api 层抛出带后端文案的 ApiError。
    const created = await apiPostForm<BackendContractUpload | null>(
        "/admin/contracts/upload",
        form,
        { timeoutMs: 60_000 },
    )
    if (!created?.id || !created.revision_id) {
        throw createApiError({
            kind: "Parse",
            message: "上传响应缺少合同或修订身份",
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
