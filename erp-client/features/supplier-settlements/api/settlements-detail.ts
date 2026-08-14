/**
 * W27 API 供应商结算 · 结算单详情查询
 * 从 api/settlements.ts 拆出；工作项校验与降级阻断语义保持不变。
 */

import { apiGet } from "@/lib/api"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import {
    mapFormalReviewTask,
    toDetail,
    type BackendDetail,
} from "@/features/supplier-settlements/api/settlements-wire"

export async function fetchSettlementDetail(input: {
    statementId: string
    workItemId?: string
}): Promise<SettlementDetailView> {
    const detail = await apiGet<BackendDetail>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}`,
    )
    const embeddedTask = detail.review_work_item
    const formalTask =
        embeddedTask &&
        (!input.workItemId || embeddedTask.work_item_id === input.workItemId)
            ? mapFormalReviewTask(embeddedTask, input.statementId)
            : undefined
    const workItemBlocker =
        input.workItemId && embeddedTask?.work_item_id !== input.workItemId
            ? {
                  action: "REVIEW_DECISION",
                  code: "FORMAL_REVIEW_WORK_ITEM_NOT_ACCESSIBLE",
                  message:
                      "指定任务与当前结算单或 W27 路由不匹配，禁止降级为对象直接确认。",
              }
            : detail.review_action_blockers?.[0]
    return toDetail(detail, formalTask, workItemBlocker)
}
