/**
 * W14 基础资料 · 稳定 HTTP 入口。
 * 列表/详情查询实现见 api/resource-queries；本文件只做再导出。
 */

export {
    fetchProductFilterOptions,
    fetchProductListSkus,
    listSellableItemsPage,
    updateProductListingStatus,
} from "@/features/master-data/api/lists"
export {
    createMasterDataObject,
    createMasterDataRevision,
    disableMasterDataObject,
    revealMasterDataSensitive,
} from "@/features/master-data/api/mutations"
export {
    fetchMasterDataCenter,
    fetchMasterDataList,
    fetchSkuSupplierCounts,
} from "@/features/master-data/api/resource-queries"
