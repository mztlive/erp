/** 供应商对象中心：聚合 Party / 联系人 / 银行 / 税号 / 能力 / 资质 / 评分。 */

import { apiGet } from "@/lib/api"
import type {
    SupplierDetailDto,
    SupplierQualificationDto,
} from "@/features/master-data/api/contracts"
import { mapSupplierRow } from "@/features/master-data/api/list-mappers"
import { resolveMediaAssets } from "@/features/master-data/api/media-assets"
import {
    asLifecycle,
    capabilityLabel,
    fact,
    factsOf,
    invoiceLabel,
    isApiError,
    parseBusinessCategoryFromSnapshot,
    pickDefaultOrFirst,
    ratingLabel,
    settlementLabel,
    taxRatePercent,
    tsToIso,
} from "@/features/master-data/api/presentation"
import type {
    MasterDataCenterView,
    RevisionTimelineEntry,
} from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerSupplier(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    let detail: SupplierDetailDto
    try {
        detail = await apiGet<SupplierDetailDto>(`/admin/suppliers/${stableId}`)
    } catch (error) {
        if (isApiError(error) && error.status === 404) return null
        throw error
    }

    const profile = detail.current_profile
    const contacts = detail.contacts
    const banks = detail.bank_accounts
    const taxProfiles = detail.tax_profiles
    const capabilities = detail.capabilities
    const qualifications = detail.qualifications
    const ratings = detail.ratings
    const profiles = detail.commercial_profiles
    const partyName =
        detail.legal_name ||
        detail.short_name ||
        detail.party_no ||
        detail.supplier_no
    const row = mapSupplierRow(detail, partyName, profile)

    const contact = pickDefaultOrFirst(contacts)
    const bank = pickDefaultOrFirst(banks)
    const taxProfile = pickDefaultOrFirst(taxProfiles)
    const sortedRatings = [...ratings].sort(
        (a, b) => (b.revision_no ?? 0) - (a.revision_no ?? 0),
    )
    const rating = sortedRatings[0]
    const initialRating = [...sortedRatings]
        .reverse()
        .find((item) => item.initial_score != null)
    const invoiceTaxRatePercent = taxRatePercent(profile?.invoice_tax_rate)

    const capabilityLabels = capabilities
        .map((c) => capabilityLabel(c.capability_code))
        .filter(Boolean)
        .join("、")
    const capabilityCodeById = new Map(
        capabilities.map((capability) => [
            capability.id,
            capability.capability_code,
        ]),
    )
    const qualificationCapabilityCodes = Object.fromEntries(
        qualifications.map((qualification) => [
            `${qualification.qualification_type}::${qualification.certificate_no}`,
            qualification.capability_ids.flatMap((id) => {
                const code = capabilityCodeById.get(id)
                return code ? [code] : []
            }),
        ]),
    )

    // 经营类目：独立字段优先；兼容历史付款条件快照编码与早期 fulfillment_note
    const businessCategory =
        profile?.business_category?.trim() ||
        parseBusinessCategoryFromSnapshot(profile?.payment_term_snapshot) ||
        capabilities.map((c) => c.fulfillment_note?.trim()).find(Boolean) ||
        ""

    const qualByType = (type: string) =>
        qualifications.find((q) => q.qualification_type === type)

    // 资质附件：解析 asset → 文件清单（fileName/assetId/url），供回显链接与编辑回填
    const qualGroups = new Map<string, SupplierQualificationDto[]>()
    for (const q of qualifications) {
        const list = qualGroups.get(q.qualification_type) ?? []
        list.push(q)
        qualGroups.set(q.qualification_type, list)
    }
    const qualAssets = await resolveMediaAssets(
        qualifications
            .map((q) => q.attachment_id)
            .filter((id): id is string => Boolean(id?.trim())),
    )
    const qualFieldEntries = (
        type: string,
    ): { fileName: string; assetId: string; url: string }[] =>
        (qualGroups.get(type) ?? []).flatMap((q) => {
            const asset = q.attachment_id
                ? qualAssets.get(q.attachment_id)
                : null
            if (!q.attachment_id) return []
            return [
                {
                    fileName: asset?.file_name ?? q.certificate_no,
                    assetId: q.attachment_id,
                    url: asset?.public_url ?? "",
                },
            ]
        })
    const qualFileNames = (type: string): string =>
        qualFieldEntries(type)
            .map((entry) => entry.fileName)
            .join(", ")

    const contractQual = qualByType("contract")
    const authQual = qualByType("authorization")

    // 标签必须与 RESOURCE_FIELDS.suppliers / masterDataCopy 一致，供编辑回填
    const facts = factsOf(
        fact("供应商编号", detail.supplier_no),
        fact("企业主体", partyName),
        fact("统一社会信用代码", detail.unified_credit_code),
        fact("联系人", contact?.contact_name),
        // mobile 不在列表契约中；telephone 若创建时同步写入可回显
        fact("联系电话", contact?.telephone),
        fact("结算方式", settlementLabel(profile?.settlement_mode)),
        fact("发票类型", invoiceLabel(profile?.invoice_type)),
        fact("发票税点", invoiceTaxRatePercent),
        fact("能力", capabilityLabels),
        fact("经营类目", businessCategory || null),
        fact("公司签约主体", profile?.signing_entity_party_id),
        fact("公司付款主体", profile?.payment_entity_party_id),
        // 标签必须与 masterDataCopy / RESOURCE_FIELDS.suppliers 完全一致
        fact("资质附件", qualFileNames("certificate") || null),
        fact("合同编号", contractQual?.certificate_no),
        fact("合同有效期起", contractQual?.valid_from),
        fact("合同有效期止", contractQual?.valid_to),
        fact("合同文件", qualFileNames("contract") || null),
        fact("授权书文件", qualFileNames("authorization") || null),
        fact("授权书有效期起", authQual?.valid_from),
        fact("授权书有效期止", authQual?.valid_to),
        fact("食品经营许可证", qualFileNames("food_license") || null),
        fact("供应商法人身份证", qualFileNames("legal_person_id") || null),
        fact("税号", taxProfile?.tax_no),
        fact("开户银行", bank?.bank_name),
        // 银行账号明文不在列表契约中，无法回显
        fact("供应商评级", ratingLabel(rating?.rating)),
        fact(
            "合作期初评分",
            initialRating?.initial_score != null
                ? String(initialRating.initial_score)
                : null,
        ),
        fact(
            "合作中评分",
            rating?.current_score != null ? String(rating.current_score) : null,
        ),
    )

    // 展示用摘要（含无值占位），与编辑 fields 分离
    const displayFacts = [
        { label: "供应商编号", value: detail.supplier_no },
        { label: "企业主体", value: partyName || "—" },
        { label: "联系人", value: contact?.contact_name || "—" },
        { label: "联系电话", value: contact?.telephone || "—" },
        {
            label: "结算方式",
            value: settlementLabel(profile?.settlement_mode) || "—",
        },
        {
            label: "发票类型",
            value: invoiceLabel(profile?.invoice_type) || "—",
        },
        {
            label: "发票税点",
            value: invoiceTaxRatePercent ? `${invoiceTaxRatePercent}%` : "—",
        },
        { label: "能力", value: capabilityLabels || "—" },
        {
            label: "资质",
            value:
                qualifications.length > 0 ? `${qualifications.length} 项` : "—",
        },
        {
            label: "供应商评级",
            value: ratingLabel(rating?.rating) || "—",
        },
        {
            label: "税号",
            value: taxProfile?.tax_no ?? "—",
        },
        { label: "开户银行", value: bank?.bank_name || "—" },
    ]

    const timeline: RevisionTimelineEntry[] = profiles.map((p, index) => ({
        id: p.id,
        revisionNo: p.revision_no,
        revisionTiming:
            index === 0 ? ("CURRENT" as const) : ("HISTORICAL" as const),
        timingLabel: index === 0 ? "当前生效" : "已结束",
        nameSnapshot: partyName,
        actor: "—",
        effectiveFrom: tsToIso(p.created_at).slice(0, 10),
        changeReason: p.change_reason,
        isCurrent: index === 0,
        lifecycleAtRevision: asLifecycle(detail.status),
    }))

    return baseCenter("suppliers", row, {
        partyLockVersion: detail.party_version ?? undefined,
        supplierQualificationCapabilityCodes: qualificationCapabilityCodes,
        resourceFacts: displayFacts,
        currentRevision: {
            revisionId: profile?.id ?? detail.id,
            revisionNo: profile?.revision_no ?? detail.version,
            name: partyName,
            effectiveFrom: tsToIso(
                profile?.created_at ?? detail.created_at,
            ).slice(0, 10),
            changeReason: profile?.change_reason ?? "—",
            actor: "—",
            // 编辑回填专用：完整字段 + 真实值（无「—」占位）
            fields: facts,
        },
        mediaAssets: {
            qualification: qualFieldEntries("certificate"),
            contractFile: qualFieldEntries("contract"),
            authorizationFile: qualFieldEntries("authorization"),
            foodLicense: qualFieldEntries("food_license"),
            legalPersonIdCard: qualFieldEntries("legal_person_id"),
        },
        revisionTimeline:
            timeline.length > 0
                ? timeline
                : baseCenter("suppliers", row).revisionTimeline,
        sensitiveFields: detail.sensitive_fields.map((field) => ({
            label: field.label,
            maskedValue: field.masked_value,
            revealToken: field.reveal_token,
            visibility: "masked" as const,
        })),
    })
}
