import { apiGet, apiPost, createApiError } from "@/lib/api"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { contractPdfError } from "@/features/contracts/lib/pdf"

import {
    loadCustomerBrief,
    paymentTermCodeFromLabel,
    tsToIso,
    uploadFileAsset,
} from "@/features/contracts/api/helpers"
import type {
    BackendContractDetail,
    BackendContractView,
} from "@/features/contracts/api/wire-types"
import type {
    UploadContractPdfInput,
    UploadContractPdfResult,
} from "@/features/contracts/types"

/**
 * 上传合同 PDF：先 file-asset 上传，再 create contract。
 */
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

    // 后端文件资产上限 5 MiB（handler），前端文案仍为 20 MB 校验；超 5 MiB 由后端拒绝
    if (!input.customerId?.trim()) {
        throw createApiError({
            kind: "Validation",
            message: "请选择客户",
            status: 400,
            retryable: false,
        })
    }

    const customer = await loadCustomerBrief(input.customerId.trim())
    if (!customer) {
        throw createApiError({
            kind: "Http",
            status: 404,
            message: "客户不存在或无权访问",
            retryable: false,
        })
    }

    // 结算主体：以表单选定为准；未提供时退回客户自有主体。
    // 不能用名称反查：/admin/parties 的 keyword 仅匹配主体编号，名称搜索
    // 永远落空，曾导致所选结算主体被静默替换为客户自有主体。
    const settlementPartyId =
        input.settlementPartyId?.trim() || customer.partyId

    const termCode = paymentTermCodeFromLabel(input.paymentTerms)
    const termName =
        PAYMENT_TERM_OPTIONS.find((o) => o.value === termCode)?.label ??
        input.paymentTerms

    const asset = await uploadFileAsset(input.pdfFile)

    const created = await apiPost<BackendContractView>("/admin/contracts", {
        contract_no: input.contractNo.trim(),
        customer_id: input.customerId.trim(),
        settlement_party_id: settlementPartyId,
        contract_pdf_file_id: asset.id,
        archive_source: "CONTRACT_CENTER",
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
    })

    let revisionId = created.current_revision_id ?? created.id
    let revisionNo = 1
    try {
        const detail = await apiGet<BackendContractDetail>(
            `/admin/contracts/${created.id}`,
        )
        const current =
            detail.revisions.find((r) => r.id === detail.current_revision_id) ??
            detail.revisions[0]
        if (current) {
            revisionId = current.id
            revisionNo = current.revision_no
        }
    } catch {
        // keep defaults
    }

    return {
        contractId: created.id,
        contractNo: created.contract_no,
        revisionId,
        revisionNo,
        uploadedAt: tsToIso(created.created_at),
        fileName: asset.file_name,
        reference: `CT-UP-${created.contract_no}`,
    }
}
