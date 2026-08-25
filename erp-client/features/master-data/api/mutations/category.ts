/** 商品分类的创建 / 原子修订 / 停用命令。 */

import { apiPost, apiPut } from "@/lib/api"
import type { ProductCategoryDto } from "@/features/master-data/api/contracts"
import { isoNow } from "@/features/master-data/api/presentation"
import type {
    CategoryFields,
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
} from "@/features/master-data/types"
import { mapMutationError, mapProductKindInput } from "./shared"

export async function createCategory(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    const fields = input.fields as CategoryFields
    try {
        const created = await apiPost<ProductCategoryDto>(
            "/admin/product-categories",
            {
                category_code: fields.code,
                parent_category_id: fields.parentId || null,
                name: input.name.trim(),
                product_kind: mapProductKindInput(fields.productKind),
                status: "active",
            },
        )
        return {
            outcome: "succeeded",
            stableId: created.id,
            stableNo: created.category_code,
            revisionId: created.id,
            revisionNo: created.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason || "新建",
            reference: `MD-CREATE-${created.category_code}`,
            nextActions: ["查看详情", "更新资料"],
        }
    } catch (error) {
        return mapMutationError(error)
    }
}

export async function updateCategoryRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    try {
        const fields = input.fields as CategoryFields
        const updated = await apiPut<ProductCategoryDto>(
            `/admin/product-categories/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                name: input.name.trim(),
                product_kind: fields.productKind
                    ? mapProductKindInput(fields.productKind)
                    : undefined,
                status: undefined,
                parent_change:
                    fields.parentId !== undefined
                        ? { parent_category_id: fields.parentId || null }
                        : undefined,
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.category_code,
            revisionId: updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-REV-${updated.category_code}-v${updated.version}`,
            nextActions: ["查看变更历史", "返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}

export async function disableCategory(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    try {
        const updated = await apiPut<ProductCategoryDto>(
            `/admin/product-categories/${input.stableId}`,
            {
                version: input.expectedLockVersion,
                status: "disabled",
            },
        )
        return {
            outcome: "succeeded",
            stableId: updated.id,
            stableNo: updated.category_code,
            revisionId: updated.id,
            revisionNo: updated.version,
            revisionState: "CURRENT",
            effectiveFrom: input.effectiveFrom,
            recordedAt: isoNow(),
            actor: "—",
            changeReason: input.changeReason,
            reference: `MD-DIS-${updated.category_code}`,
            nextActions: ["返回列表"],
        }
    } catch (error) {
        return mapMutationError(error, {
            version: input.expectedLockVersion,
            revisionNo: 0,
        })
    }
}
