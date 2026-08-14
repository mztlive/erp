/** 供应商的创建 / 修订 / 停用命令（走 supplier-profiles 根命令）。 */

import { apiPost, apiPut } from "@/lib/api"
import type {
    SupplierDto,
    SupplierProfileMutationDto,
} from "@/features/master-data/api/contracts"
import {
    buildPaymentTermSnapshot,
    capabilityToBackend,
    genBusinessCode,
    invoiceToBackend,
    isoNow,
    normalizeTaxRate,
    parseScore100,
    ratingToBackend,
    settlementToBackend,
    todayDateOnly,
    tsToIso,
} from "@/features/master-data/api/presentation"
import { parseMediaList } from "@/features/master-data/lib/resource-fields"
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
    SupplierFields,
} from "@/features/master-data/types"
import { mapMutationError } from "./shared"

type SupplierProfileQualificationInput = {
    qualification_type: string
    certificate_no: string
    issuer: null
    valid_from: string
    valid_to: string | null
    attachment_id: string | null
    capability_codes: string[]
}

/** 将页面文件字段转换为根级供应商资料命令中的结构化资质集合。 */
const buildSupplierProfileQualifications = (
    fields: SupplierFields,
    effectiveFrom: string,
    capabilityCodes: string[],
): SupplierProfileQualificationInput[] => {
    const result: SupplierProfileQualificationInput[] = []
    const pushFiles = (
        qualificationType: string,
        names: string[],
        assetIds: Readonly<Record<string, string>> | undefined,
        validFrom: string,
        validTo?: string,
        certificateNo?: string,
    ) => {
        names.forEach((name, index) => {
            const attachmentId = assetIds?.[name]?.trim()
            if (!attachmentId) return
            const resolvedCertificateNo =
                certificateNo && index === 0 ? certificateNo : name
            const existingCodes =
                fields.qualificationCapabilityCodes?.[
                    `${qualificationType}::${resolvedCertificateNo}`
                ]
            result.push({
                qualification_type: qualificationType,
                certificate_no: resolvedCertificateNo,
                issuer: null,
                valid_from: validFrom,
                valid_to: validTo || null,
                attachment_id: attachmentId,
                capability_codes: existingCodes
                    ? existingCodes.filter((code) =>
                          capabilityCodes.includes(code),
                      )
                    : capabilityCodes,
            })
        })
    }
    pushFiles(
        "certificate",
        parseMediaList(fields.qualification),
        fields.qualificationFileAssetIds,
        effectiveFrom,
    )
    pushFiles(
        "contract",
        parseMediaList(fields.contractFile).slice(0, 1),
        fields.contractFileAssetIds,
        fields.contractValidFrom || effectiveFrom,
        fields.contractValidTo,
        fields.contractNo?.trim() || "CONTRACT",
    )
    pushFiles(
        "authorization",
        parseMediaList(fields.authorizationFile),
        fields.authorizationFileAssetIds,
        fields.authorizationValidFrom || effectiveFrom,
        fields.authorizationValidTo,
    )
    pushFiles(
        "food_license",
        parseMediaList(fields.foodLicense),
        fields.foodLicenseFileAssetIds,
        effectiveFrom,
    )
    pushFiles(
        "legal_person_id",
        parseMediaList(fields.legalPersonIdCard),
        fields.legalPersonIdCardFileAssetIds,
        effectiveFrom,
    )
    return result
}

const supplierCapabilityCodes = (fields: SupplierFields): string[] =>
    (fields.capability ?? "")
        .split(/[、,，]/)
        .map((value) => capabilityToBackend(value.trim()))
        .filter((value): value is string => Boolean(value))

export async function createSupplier(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as SupplierFields
    const capabilityCodes = supplierCapabilityCodes(fields)
    const effectiveFrom = input.effectiveFrom || todayDateOnly()
    if (!fields.signingEntity?.trim() || !fields.paymentEntity?.trim()) {
        return {
            outcome: "blocked",
            code: "SUPPLIER_INTERNAL_PARTY_REQUIRED",
            message: "请选择公司签约主体和公司付款主体。",
        }
    }
    try {
        const created = await apiPost<SupplierProfileMutationDto>(
            "/admin/supplier-profiles",
            {
                idempotency_key: input.idempotencyKey,
                party_no: genBusinessCode("PTY"),
                supplier_no: genBusinessCode("SUP"),
                expected_party_version: null,
                expected_supplier_version: null,
                legal_name: fields.company || input.name.trim(),
                short_name: input.name.trim(),
                unified_credit_code: fields.creditCode?.trim() || null,
                contact:
                    fields.contactName?.trim() && fields.contactPhone?.trim()
                        ? {
                              contact_name: fields.contactName.trim(),
                              mobile: fields.contactPhone.trim(),
                              telephone: null,
                              email: null,
                          }
                        : null,
                clear_contact: false,
                address: fields.address?.trim()
                    ? {
                          address: fields.address.trim(),
                          contact_name: fields.contactName?.trim() || null,
                      }
                    : null,
                clear_address: false,
                tax_no: fields.taxNo?.trim() || null,
                clear_tax_profile: false,
                bank_account:
                    fields.bankName?.trim() && fields.bankAccount?.trim()
                        ? {
                              bank_name: fields.bankName.trim(),
                              account_number: fields.bankAccount.trim(),
                          }
                        : null,
                clear_bank_account: false,
                settlement_mode: settlementToBackend(fields.settlement),
                reconciliation_cycle: "monthly",
                payment_term_snapshot: buildPaymentTermSnapshot(
                    fields.settlement,
                    fields.businessCategory,
                ),
                invoice_type: invoiceToBackend(fields.invoiceType),
                invoice_tax_rate: normalizeTaxRate(fields.invoiceTaxRate),
                signing_entity_party_id: fields.signingEntity.trim(),
                payment_entity_party_id: fields.paymentEntity.trim(),
                capability_codes: capabilityCodes,
                qualifications: buildSupplierProfileQualifications(
                    fields,
                    effectiveFrom,
                    capabilityCodes,
                ),
                rating:
                    fields.supplierRating ||
                    fields.currentScore ||
                    fields.initialScore
                        ? {
                              initial_score:
                                  parseScore100(fields.initialScore) ?? null,
                              rating: ratingToBackend(fields.supplierRating),
                              current_score:
                                  parseScore100(fields.currentScore) ?? 0,
                              valid_from: effectiveFrom,
                          }
                        : null,
                effective_from: effectiveFrom,
                change_reason: input.changeReason || "新建",
            },
        )

        return {
            outcome: "succeeded",
            stableId: created.supplier_id,
            stableNo: created.supplier_no,
            revisionId: created.revision_id,
            revisionNo: created.revision_no,
            revisionState: "CURRENT",
            effectiveFrom: created.effective_from,
            recordedAt: tsToIso(created.recorded_at),
            actor: "—",
            changeReason: created.change_reason,
            reference: `MD-CREATE-${created.supplier_no}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

export async function updateSupplierRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    try {
        const fields = input.fields as SupplierFields
        const capabilityCodes = supplierCapabilityCodes(fields)
        const effectiveFrom = input.effectiveFrom || todayDateOnly()
        if (
            input.expectedPartyVersion == null ||
            !fields.signingEntity?.trim() ||
            !fields.paymentEntity?.trim()
        ) {
            return {
                outcome: "blocked",
                code: "SUPPLIER_PROFILE_REQUIRED_CONTEXT",
                message: "供应商版本或签约、付款主体缺失，请刷新后重试。",
            }
        }
        const updated = await apiPut<SupplierProfileMutationDto>(
            `/admin/supplier-profiles/${input.stableId}`,
            {
                idempotency_key: input.idempotencyKey,
                party_no: null,
                supplier_no: null,
                expected_party_version: input.expectedPartyVersion,
                expected_supplier_version: input.expectedLockVersion,
                legal_name: fields.company || input.name.trim(),
                short_name: input.name.trim(),
                unified_credit_code: fields.creditCode?.trim() || null,
                contact:
                    fields.contactName?.trim() && fields.contactPhone?.trim()
                        ? {
                              contact_name: fields.contactName.trim(),
                              mobile: fields.contactPhone.trim(),
                              telephone: null,
                              email: null,
                          }
                        : null,
                clear_contact: fields.clearContact === true,
                address: fields.address?.trim()
                    ? {
                          address: fields.address.trim(),
                          contact_name: fields.contactName?.trim() || null,
                      }
                    : null,
                clear_address: fields.clearAddress === true,
                tax_no: fields.taxNo?.trim() || null,
                clear_tax_profile: fields.clearTaxProfile === true,
                bank_account:
                    fields.bankName?.trim() && fields.bankAccount?.trim()
                        ? {
                              bank_name: fields.bankName.trim(),
                              account_number: fields.bankAccount.trim(),
                          }
                        : null,
                clear_bank_account: fields.clearBankAccount === true,
                settlement_mode: settlementToBackend(fields.settlement),
                reconciliation_cycle: "monthly",
                payment_term_snapshot: buildPaymentTermSnapshot(
                    fields.settlement,
                    fields.businessCategory,
                ),
                invoice_type: invoiceToBackend(fields.invoiceType),
                invoice_tax_rate: normalizeTaxRate(fields.invoiceTaxRate),
                signing_entity_party_id: fields.signingEntity.trim(),
                payment_entity_party_id: fields.paymentEntity.trim(),
                capability_codes: capabilityCodes,
                qualifications: buildSupplierProfileQualifications(
                    fields,
                    effectiveFrom,
                    capabilityCodes,
                ),
                rating:
                    fields.supplierRating ||
                    fields.currentScore ||
                    fields.initialScore
                        ? {
                              initial_score:
                                  parseScore100(fields.initialScore) ?? null,
                              rating: ratingToBackend(fields.supplierRating),
                              current_score:
                                  parseScore100(fields.currentScore) ?? 0,
                              valid_from: effectiveFrom,
                          }
                        : null,
                effective_from: effectiveFrom,
                change_reason: input.changeReason,
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.supplier_id,
            stableNo: updated.supplier_no,
            revisionId: updated.revision_id,
            revisionNo: updated.revision_no,
            revisionState: "CURRENT",
            effectiveFrom: updated.effective_from,
            recordedAt: tsToIso(updated.recorded_at),
            actor: "—",
            changeReason: updated.change_reason,
            reference: `MD-REV-${updated.supplier_no}-v${updated.revision_no}`,
            nextActions: ["查看变更历史", "返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableSupplier(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    try {
        const updated = await apiPut<SupplierDto>(
            `/admin/suppliers/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                status: "disabled",
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.supplier_no,
            revisionId:
                updated.current_commercial_profile_revision_id ?? updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-DIS-${updated.supplier_no}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}
