"use client"

import {
    useMallSyncPageData,
    type UseMallSyncPageInput,
} from "@/features/mall-sync/pages/hooks/use-mall-sync-page-data"
import { useMallSyncActionFeedback } from "@/features/mall-sync/pages/hooks/use-mall-sync-action-feedback"
import { useMallSyncMappingActions } from "@/features/mall-sync/pages/hooks/use-mall-sync-mapping-actions"
import { useMallSyncManualSyncActions } from "@/features/mall-sync/pages/hooks/use-mall-sync-manual-sync-actions"

/**
 * 商城同步页控制 hook：组合数据查询、动作反馈、映射动作与手动同步动作，
 * 页面组件只消费本 hook 的返回值渲染 UI。
 */
export function useMallSyncPage(input: UseMallSyncPageInput) {
    const data = useMallSyncPageData(input)
    const feedback = useMallSyncActionFeedback()
    const mapping = useMallSyncMappingActions(data, feedback, input.patchUrl)
    const manual = useMallSyncManualSyncActions(data, feedback, input.patchUrl)
    return { ...data, ...feedback, ...mapping, ...manual }
}

export type { UseMallSyncPageInput } from "@/features/mall-sync/pages/hooks/use-mall-sync-page-data"

export type MallSyncPageApi = ReturnType<typeof useMallSyncPage>
export type MallSyncConfirmFormApi = MallSyncPageApi["confirmForm"]
export type MallSyncSourceFixFormApi = MallSyncPageApi["sourceFixForm"]
export type MallSyncReleaseFormApi = MallSyncPageApi["releaseForm"]
export type MallSyncPullFormApi = MallSyncPageApi["pullForm"]
export type MallSyncIncrementalFormApi = MallSyncPageApi["incrementalForm"]
