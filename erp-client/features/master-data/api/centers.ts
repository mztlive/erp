/**
 * W14 基础资料 · 对象中心（详情）适配。
 *
 * 按资源拆分为 api/centers/<resource>，公共骨架在 api/centers/base；
 * 本文件只做再导出，既有导入路径保持不变。
 */

export { baseCenter } from "./centers/base"
export { centerCategory } from "./centers/category"
export { centerBrand } from "./centers/brand"
export { centerUnitOfMeasure } from "./centers/unit-of-measure"
export { centerProduct, parseSpecificationSignature } from "./centers/product"
export { centerSellable } from "./centers/sellable"
export { centerVoucher } from "./centers/voucher"
export { centerWarehouse } from "./centers/warehouse"
export { centerSupplier } from "./centers/supplier"
