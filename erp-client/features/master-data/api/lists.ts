/**
 * W14 基础资料 · 列表查询适配。
 *
 * 按资源拆分为 api/lists/<resource>，通用分页在 api/lists/fetch-all；
 * 本文件只做再导出，既有导入路径保持不变。
 */

export { fetchAllPages } from "./lists/fetch-all"
export {
    listBrands,
    listCategories,
    listUnitOfMeasures,
} from "./lists/dictionaries"
export {
    fetchProductFilterOptions,
    fetchProductListSkus,
    listProducts,
    updateProductListingStatus,
} from "./lists/products"
export { listSellableItems } from "./lists/sellable"
export { listVoucherCategories } from "./lists/voucher"
export { listWarehouses } from "./lists/warehouse"
export { joinFilterCodes, listSuppliers } from "./lists/supplier"
