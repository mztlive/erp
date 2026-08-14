/** 公司商品池只读投影：所有写命令按业务规则直接拒绝。 */

import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataMutationResult,
    SellableItemFields,
} from "@/features/master-data/types"

export async function createSellable(
    input: CreateMasterDataInput,
): Promise<MasterDataMutationResult> {
    // Sellable pool is a projection over company SKUs; not an independent create target.
    // Treat as create product with single SKU is wrong domain. Block with guidance.
    const fields = input.fields as SellableItemFields
    void fields
    return {
        outcome: "blocked",
        code: "SELLABLE_NOT_WRITABLE",
        message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
        detail: "W14：sellable-items 不是独立 resource 写入口。",
    }
}

export async function updateSellableRevision(
    _input: CreateRevisionInput,
): Promise<MasterDataMutationResult> {
    return {
        outcome: "blocked",
        code: "SELLABLE_NOT_WRITABLE",
        message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
    }
}

export async function disableSellable(
    _input: DisableMasterDataInput,
): Promise<MasterDataMutationResult> {
    return {
        outcome: "blocked",
        code: "SELLABLE_NOT_WRITABLE",
        message: "公司商品池是销售可见 SKU 投影，请在「商品与 SKU」中维护。",
    }
}
