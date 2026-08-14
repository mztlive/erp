/** 卡券类目的创建 / 修订命令；停用按业务规则直接拒绝。 */

import { apiPost, apiPut } from "@/lib/api"
import type { VoucherCategoryProfileDto } from "@/features/master-data/api/contracts"
import { isFutureDate } from "@/features/master-data/api/list-mappers"
import { isoNow } from "@/features/master-data/api/presentation"
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
    VoucherCategoryFields,
} from "@/features/master-data/types"
import { mapMutationError } from "./shared"

export async function createVoucherCategory(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as VoucherCategoryFields
    // 分类 / 品牌 / 单位均可省略：后端补齐共用卡券根分类、品牌「福尚云」、单位「张」。
    // 若调用方仍传入 categoryId / newCategory / brandId / baseUnitId，则原样转发覆盖默认。
    const body: Record<string, unknown> = {
        voucher_no: fields.voucherNo,
        name: input.name.trim(),
        description: (fields.description || input.name).trim(),
        specification: fields.specification || null,
        status: "active",
        effective_from: input.effectiveFrom || null,
        effective_to: input.effectiveTo || null,
    }
    if (fields.categoryId) {
        body.category_id = fields.categoryId
    } else if (fields.newCategoryCode && fields.newCategoryName) {
        body.new_category = {
            category_code: fields.newCategoryCode,
            parent_category_id: fields.newCategoryParentId || null,
            name: fields.newCategoryName,
        }
    }
    if (fields.brandId) {
        body.brand_id = fields.brandId
    }
    if (fields.baseUnitId) {
        body.sku = {
            base_unit_id: fields.baseUnitId,
            barcode: fields.barcode || null,
            weight_kg: null,
            volume_m3: null,
            sales_visible_price_gross: fields.salesVisiblePriceGross || null,
            market_price: fields.marketPrice || null,
        }
    }
    try {
        const created = await apiPost<VoucherCategoryProfileDto>(
            "/admin/voucher-categories",
            body,
        )
        return {
            outcome: "succeeded",
            stableId: created.sku_id,
            stableNo: created.sku_no ?? fields.voucherNo,
            revisionId: created.id,
            revisionNo: created.revision_no,
            revisionState: isFutureDate(input.effectiveFrom)
                ? "FUTURE"
                : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-VC-${fields.voucherNo}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

export async function updateVoucherCategoryRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as VoucherCategoryFields
    try {
        const updated = await apiPut<VoucherCategoryProfileDto>(
            `/admin/voucher-categories/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                name: input.name.trim(),
                description: (fields.description || input.name).trim(),
                effective_from: input.effectiveFrom || null,
                effective_to: input.effectiveTo || null,
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.sku_id,
            stableNo: updated.sku_no ?? fields.voucherNo ?? input.stableId,
            revisionId: updated.id,
            revisionNo: updated.revision_no,
            revisionState: isFutureDate(input.effectiveFrom)
                ? "FUTURE"
                : "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "更新",
            reference: `MD-REV-VC-${updated.sku_no ?? input.stableId}-v${updated.revision_no}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableVoucherCategory(
    _input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    return {
        outcome: "blocked",
        code: "VOUCHER_NO_DISABLE",
        message: "卡券类目不支持停用。",
    }
}
