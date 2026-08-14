/**
 * W14 基础资料 · 变更命令入口。
 *
 * 各资源的创建 / 修订 / 停用实现拆分到 api/mutations/<resource>，
 * 守卫与错误映射在 api/mutations/shared；本文件只做资源分派。
 */

import { apiPost } from "@/lib/api"
import { resourceLabel } from "@/features/master-data/lib/data"
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
} from "@/features/master-data/types"
import { blockedWarehouse } from "./mutations/shared"
import {
    createCategory,
    disableCategory,
    updateCategoryRevision,
} from "./mutations/category"
import {
    createBrand,
    disableBrand,
    updateBrandRevision,
} from "./mutations/brand"
import {
    createUnitOfMeasure,
    disableUnitOfMeasure,
    updateUnitOfMeasureRevision,
} from "./mutations/unit-of-measure"
import {
    createProduct,
    disableProduct,
    updateProductRevision,
} from "./mutations/product"
import {
    createVoucherCategory,
    disableVoucherCategory,
    updateVoucherCategoryRevision,
} from "./mutations/voucher"
import {
    createSupplier,
    disableSupplier,
    updateSupplierRevision,
} from "./mutations/supplier"
import {
    createSellable,
    disableSellable,
    updateSellableRevision,
} from "./mutations/sellable"

export async function createMasterDataObject(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    if (input.resource === "warehouses") return blockedWarehouse()
    switch (input.resource) {
        case "categories":
            return createCategory(input)
        case "brands":
            return createBrand(input)
        case "unit-of-measures":
            return createUnitOfMeasure(input)
        case "products":
            return createProduct(input)
        case "voucher-categories":
            return createVoucherCategory(input)
        case "suppliers":
            return createSupplier(input)
        case "sellable-items":
            return createSellable(input)
        default:
            return {
                outcome: "blocked",
                code: "UNSUPPORTED_RESOURCE",
                message: `暂不支持新建资源：${resourceLabel(input.resource)}`,
            }
    }
}

export async function createMasterDataRevision(
    input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    if (input.resource === "warehouses") return blockedWarehouse()

    switch (input.resource) {
        case "categories":
            return updateCategoryRevision(input)
        case "brands":
            return updateBrandRevision(input)
        case "unit-of-measures":
            return updateUnitOfMeasureRevision(input)
        case "products":
            return updateProductRevision(input)
        case "suppliers":
            return updateSupplierRevision(input)
        case "sellable-items":
            return updateSellableRevision(input)
        case "voucher-categories":
            return updateVoucherCategoryRevision(input)
        default:
            return {
                outcome: "blocked",
                code: "UNSUPPORTED_RESOURCE",
                message: `暂不支持更新资源：${resourceLabel(input.resource)}`,
            }
    }
}

export async function disableMasterDataObject(
    input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    if (input.resource === "warehouses") return blockedWarehouse()

    switch (input.resource) {
        case "categories":
            return disableCategory(input)
        case "brands":
            return disableBrand(input)
        case "unit-of-measures":
            return disableUnitOfMeasure(input)
        case "products":
            return disableProduct(input)
        case "suppliers":
            return disableSupplier(input)
        case "voucher-categories":
            return disableVoucherCategory(input)
        case "sellable-items":
            return disableSellable(input)
        default:
            return {
                outcome: "blocked",
                code: "UNSUPPORTED_RESOURCE",
                message: `暂不支持停用资源：${resourceLabel(input.resource)}`,
            }
    }
}

/** 使用短期令牌揭示供应商敏感字段；服务端再次执行权限校验并记录审计。 */
export async function revealMasterDataSensitive(
    revealToken: string,
): Promise<string> {
    const result = await apiPost<{ value: string }>(
        "/admin/supplier-sensitive-fields/reveal",
        { reveal_token: revealToken },
    )
    return result.value
}
