/**
 * W30 历史消费回填 · 枚举中文映射与阶段表
 * 从 types.ts 拆出以控制文件体量；类型契约仍在 types.ts。
 */

import type { ImportStageKey } from "@/components/business"
import type {
    BackfillPipelineStage,
    CostBasis,
    HistoryBackfillEnvironment,
    HistoryBackfillProcessingStatus,
    HistoryBackfillReportReviewStatus,
    HistoryBackfillView,
    ItemResult,
    MallOrderFactType,
} from "@/features/history-backfill/types"

export const PIPELINE_TO_INDICATOR: Record<
    BackfillPipelineStage,
    ImportStageKey
> = {
    SCOPE: "upload",
    VALIDATE_SOURCE: "mapping",
    INGEST: "validation",
    ATTRIBUTE: "preview",
    REPORT: "submission",
    DONE: "result",
}

export const PIPELINE_STAGE_LABEL: Record<BackfillPipelineStage, string> = {
    SCOPE: "范围确认",
    VALIDATE_SOURCE: "来源校验",
    INGEST: "记录入库",
    ATTRIBUTE: "归集评估",
    REPORT: "报告",
    DONE: "完成",
}

export const PIPELINE_ORDER: BackfillPipelineStage[] = [
    "SCOPE",
    "VALIDATE_SOURCE",
    "INGEST",
    "ATTRIBUTE",
    "REPORT",
    "DONE",
]

export const PROCESSING_STATUS_LABEL: Record<
    HistoryBackfillProcessingStatus,
    string
> = {
    DRAFT: "待执行",
    VALIDATING: "校验中",
    READY: "可执行",
    RUNNING: "运行中",
    PARTIAL: "部分完成",
    COMPLETED: "技术处理完成",
    FAILED: "失败",
}

export const PROCESSING_STATUS_TONE: Record<
    HistoryBackfillProcessingStatus,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    DRAFT: "neutral",
    VALIDATING: "info",
    READY: "info",
    RUNNING: "info",
    PARTIAL: "warning",
    COMPLETED: "success",
    FAILED: "destructive",
}

export const REPORT_REVIEW_STATUS_LABEL: Record<
    HistoryBackfillReportReviewStatus,
    string
> = {
    NOT_READY: "未就绪",
    POLICY_NOT_CONFIGURED: "策略未配置",
    PENDING: "待确认",
    CONFIRMED: "已确认",
    REJECTED: "已驳回",
}

export const REPORT_REVIEW_STATUS_TONE: Record<
    HistoryBackfillReportReviewStatus,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    NOT_READY: "neutral",
    POLICY_NOT_CONFIGURED: "warning",
    PENDING: "warning",
    CONFIRMED: "success",
    REJECTED: "destructive",
}

export const FACT_TYPE_LABEL: Record<MallOrderFactType, string> = {
    PAYMENT_SUCCEEDED: "支付成功",
    ORDER_CANCELED: "订单取消",
    REFUND_SUCCEEDED: "退款成功",
    ORDER_COMPLETED: "订单完成",
    CARD_BALANCE_RESTORED: "余额恢复",
}

export const ITEM_RESULT_LABEL: Record<ItemResult, string> = {
    INSERTED: "新增业务记录",
    DEDUPLICATED: "重叠去重",
    UNATTRIBUTED: "待归集",
    FAILED: "处理失败",
}

export const ITEM_RESULT_TONE: Record<
    ItemResult,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    INSERTED: "success",
    DEDUPLICATED: "info",
    UNATTRIBUTED: "warning",
    FAILED: "destructive",
}

export const COST_BASIS_LABEL: Record<CostBasis, string> = {
    ACTUAL: "实际成本",
    STANDARD: "时点标准成本",
    NONE: "未覆盖",
}

/** 失败明细错误码中文映射；仅在后端未返回 error_detail 时兜底，禁止原码上屏。 */
export const FAILURE_CODE_LABEL: Record<string, string> = {
    SOURCE_SCHEMA_FIELD_MISSING: "来源字段缺失",
    TAX_BASIS_UNRESOLVED: "税口径无法解析",
}

export const ENVIRONMENT_LABEL: Record<HistoryBackfillEnvironment, string> = {
    production: "生产环境",
    verification: "验证环境",
}

export const VIEW_LABEL: Record<HistoryBackfillView, string> = {
    active: "活跃任务",
    processing_completed: "技术处理完成",
    report_pending: "报告待确认",
    all: "全部",
}
