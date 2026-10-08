/**
 * W29 用户可见中文标签映射（术语表校验口径）。
 * 从 types.ts 拆出以聚焦类型声明；types.ts 统一再导出，导入路径不变。
 */

import type {
    ControlledEvidenceKind,
    FundsImpact,
    IntegrationEnvironment,
    IntegrationMode,
    IntegrationView,
} from "../types"

export const ERROR_CLASS_LABEL: Record<string, string> = {
    "capability-unsupported": "能力不足",
    "parameter-or-mapping": "参数/映射错误",
    "business-rejected": "供应商业务拒绝",
    "network-timeout": "临时故障",
    "result-unknown": "结果未知",
    "authentication-or-signature": "鉴权/签名失败",
    "rate-limited": "调用次数受限",
    "duplicate-callback": "重复通知",
    "out-of-order-callback": "通知顺序异常",
    "reconciliation-difference": "对账差异",
}

export const VIEW_LABEL: Record<IntegrationView, string> = {
    mine: "我的任务",
    result_unknown: "结果未知",
    security: "安全故障",
    auto_retry: "自动重试",
    reconciliation: "对账差异",
    resolved: "已解决",
}

export const MODE_LABEL: Record<IntegrationMode, string> = {
    all: "全部",
    errors: "错误任务",
}

export const ENV_LABEL: Record<IntegrationEnvironment | "all", string> = {
    all: "全部环境",
    production: "生产",
    verification: "验证",
}

export const FUNDS_LABEL: Record<FundsImpact, string> = {
    NONE: "无资金影响",
    POTENTIAL: "潜在资金影响",
    POSTED: "已入账资金",
}

export const EVIDENCE_KIND_LABEL: Record<ControlledEvidenceKind, string> = {
    EXTERNAL_CASE_RESULT: "外部案例结果",
    BUSINESS_OBJECT_VERIFICATION: "业务对象核验",
    FINANCIAL_RECONCILIATION: "财务对账",
    COMPENSATION_RESULT: "补偿结果",
    DISTINCT_REVIEW: "独立复核",
}

/** 对账差异类型中文映射（differenceType） */
export const DIFFERENCE_TYPE_LABEL: Record<string, string> = {
    MALL_MISSING: "商城无对应记录",
    ERP_MISSING: "ERP 无对应记录",
    STATUS_DIFFERENCE: "状态不一致",
    CONTENT_FINGERPRINT_DIFFERENCE: "内容不一致",
    DUPLICATE_IDENTITY: "重复业务记录",
    AMOUNT_MISMATCH: "金额不一致",
    REFUND_MISMATCH: "退款不一致",
    BALANCE_MISMATCH: "余额不一致",
    SETTLEMENT_MISMATCH: "结算不一致",
    COST_MISMATCH: "成本不一致",
    SUPPLIER_SUPPLY_MISMATCH: "供应商供给不一致",
    SUPPLIER_ORDER_MISMATCH: "供应商订单不一致",
    SUPPLY_MISMATCH: "供给不一致",
    RESULT_UNKNOWN: "集成结果未知",
    INTEGRATION_RESULT_UNKNOWN: "集成结果未知",
    AMOUNT_AND_LINE_COUNT: "金额与行数差异",
    MISSING_ERP_FACT: "ERP 无对应记录",
    MISSING_MALL_FACT: "商城无对应记录",
    STATUS_MISMATCH: "状态不一致",
}

/** 岗位分离策略中文映射（reviewerSeparation） */
export const REVIEWER_SEPARATION_LABEL: Record<string, string> = {
    NONE: "无独立复核要求",
    DISTINCT_REVIEWER: "需独立复核",
    DISTINCT_FINANCE_REVIEWER: "需财务独立复核",
}
