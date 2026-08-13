/**
 * W22 · 商品发布 · 稳定 HTTP 入口。
 * 实现见 api/publications.ts；本文件只做再导出。
 */

export {
    fetchPublicationDetail,
    fetchPublicationList,
    MALLS,
    manualPausePublication,
    publishRevision,
    retryDelivery,
} from "@/features/product-publications/api/publications"
