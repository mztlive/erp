"use client"

// 兼容入口：实现已迁至 hooks/queries.ts，本文件只做再导出。
export {
    fulfillmentKeys,
    useClaimFulfillmentMutation,
    useDeferFulfillmentMutation,
    useFulfillmentCountQuery,
    useFulfillmentQueueQuery,
    usePostFulfillmentMutation,
    useResolveUnknownFulfillmentMutation,
    useSaveFulfillmentMutation,
} from "./hooks/queries"
