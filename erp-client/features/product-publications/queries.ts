/**
 * W22 · 商品发布 · 唯一对外查询/变更消费入口。
 * 实现见 hooks/queries.ts；本文件只做再导出。
 */

export {
    useManualPauseMutation,
    usePublicationDetailQuery,
    usePublicationListQuery,
    usePublishRevisionMutation,
    useRetryDeliveryMutation,
} from "@/features/product-publications/hooks/queries"
