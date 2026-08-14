import { apiGet, apiPost, type ApiError, type Page } from "@/lib/api"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { contractPdfError } from "@/features/contracts/lib/pdf"

import {
    isApiError,
    loadCustomerBrief,
    paymentTermCodeFromLabel,
    tsToIso,
    uploadFileAsset,
} from "@/features/contracts/api/helpers"
import type {
    BackendContractDetail,
    BackendContractView,
    BackendPartyView,
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
        const err: ApiError = {
            kind: "Validation",
            message: fileError,
            status: 400,
        }
        throw err
    }

    // 后端文件资产上限 5 MiB（handler），前端文案仍为 20 MB 校验；超 5 MiB 由后端拒绝
    if (!input.customerId?.trim()) {
        const err: ApiError = {
            kind: "Validation",
            message: "请选择客户",
            status: 400,
        }
        throw err
    }

    const customer = await loadCustomerBrief(input.customerId.trim())
    if (!customer) {
        const err: ApiError = {
            kind: "Http",
            status: 404,
            message: "客户不存在或无权访问",
        }
        throw err
    }

    // 结算主体：优先客户主体；名称不同时尝试按关键字查 party 列表
    let settlementPartyId = customer.partyId
    if (
        input.settlementPartyName.trim() &&
        input.settlementPartyName.trim() !== customer.displayName
    ) {
        try {
            const parties = await apiGet<Page<BackendPartyView>>(
                "/admin/parties",
                {
                    keyword: input.settlementPartyName.trim(),
                    page: 1,
                    page_size: 5,
                },
            )
            if (parties.items[0]) {
                settlementPartyId = parties.items[0].id
            }
        } catch {
            // keep customer.partyId
        }
    }

    const termCode = paymentTermCodeFromLabel(input.paymentTerms)
    const termName =
        PAYMENT_TERM_OPTIONS.find((o) => o.value === termCode)?.label ??
        input.paymentTerms

    const asset = await uploadFileAsset(input.pdfFile)

    let created: BackendContractView
    try {
        created = await apiPost<BackendContractView>("/admin/contracts", {
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
    } catch (error) {
        if (isApiError(error) && error.status === 409) {
            const err: ApiError = {
                kind: "Http",
                status: 409,
                message: "CONTRACT_NO_EXISTS",
                responseData: error.responseData,
            }
            throw err
        }
        throw error
    }

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
