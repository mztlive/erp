/** 品牌的创建 / 修订 / 停用命令（Logo 走文件资产引用）。 */

import { apiPost, apiPut } from "@/lib/api"
import type { ProductBrandDto } from "@/features/master-data/api/contracts"
import {
    postAssetCommand,
    putAssetCommand,
} from "@/features/master-data/api/pending-assets"
import { isoNow } from "@/features/master-data/api/presentation"
import type {
    BrandFields,
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
} from "@/features/master-data/types"
import { mapMutationError } from "./shared"

export async function createBrand(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as BrandFields
    try {
        const command = {
            brand_code: fields.code,
            name: input.name.trim(),
            status: "active",
            logo_file_asset_id: fields.logoAssetId || null,
        }
        const created = input.pendingAssetUploads?.length
            ? await postAssetCommand<ProductBrandDto>(
                  "/admin/product-brands/with-assets",
                  command,
                  input.pendingAssetUploads,
              )
            : await apiPost<ProductBrandDto>("/admin/product-brands", command)
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.brand_code,
            revisionId: created.id,
            revisionNo: created.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.brand_code}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

export async function updateBrandRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    try {
        const fields = input.fields as BrandFields
        const command = {
            version: input.expectedLockVersion,
            name: input.name.trim(),
            logo_file_asset_id: fields.logo ? fields.logoAssetId || null : null,
        }
        const updated = input.pendingAssetUploads?.length
            ? await putAssetCommand<ProductBrandDto>(
                  `/admin/product-brands/${input.stableId}/with-assets`,
                  command,
                  input.pendingAssetUploads,
              )
            : await apiPut<ProductBrandDto>(
                  `/admin/product-brands/${input.stableId}`,
                  command,
              )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.brand_code,
            revisionId: updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-REV-${updated.brand_code}-v${updated.version}`,
            nextActions: ["查看变更历史", "返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableBrand(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    try {
        const updated = await apiPut<ProductBrandDto>(
            `/admin/product-brands/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                status: "disabled",
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.brand_code,
            revisionId: updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-DIS-${updated.brand_code}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}
