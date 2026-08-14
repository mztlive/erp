/**
 * W22 · 商品发布 · 真实 HTTP 适配层入口。
 * 实现按资源拆分（wire-types / mappers / malls / publication-list /
 * publication-detail / publication-mutations）；本文件只做再导出，
 * 保证既有导入路径（含 MALLS 活绑定）稳定。
 */

export { MALLS } from "@/features/product-publications/api/malls"
export { fetchPublicationDetail } from "@/features/product-publications/api/publication-detail"
export { fetchPublicationList } from "@/features/product-publications/api/publication-list"
export {
    manualPausePublication,
    publishRevision,
    retryDelivery,
} from "@/features/product-publications/api/publication-mutations"
