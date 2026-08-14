/**
 * W27 供应商结算 · 枚举/动作中文映射表
 * 从 types.ts 拆出：用户可见文案统一走这些映射，枚举原值不上屏。
 * 改动文案前先查 docs/ui-glossary.md。
 */

import type { StatusTone } from "@/components/ui/status-badge"
import type {
    DifferenceResolution,
    DifferenceStatus,
    DifferenceType,
    SettlementSection,
    SettlementStatus,
    SettlementView,
} from "@/features/supplier-settlements/types"

/** 处理动作 → 结论状态（CLOSED_NO_ADJUSTMENT 动作落为 CLOSED 状态） */
export const RESOLUTION_TO_STATUS: Record<
    DifferenceResolution,
    DifferenceStatus
> = {
    SUPPLIER_ACCEPTED: "SUPPLIER_ACCEPTED",
    ERP_ACCEPTED: "ERP_ACCEPTED",
    COMPENSATED: "COMPENSATED",
    CLOSED_NO_ADJUSTMENT: "CLOSED",
}

export const STATUS_LABEL: Record<SettlementStatus, string> = {
    DRAFT: "草稿",
    PENDING_RECONCILE: "待对账",
    HAS_DIFFERENCE: "有差异",
    PENDING_REVIEW: "待复核",
    CONFIRMED: "已确认",
    VOIDED: "已作废",
}

export const STATUS_TONE: Record<SettlementStatus, StatusTone> = {
    DRAFT: "neutral",
    PENDING_RECONCILE: "info",
    HAS_DIFFERENCE: "warning",
    PENDING_REVIEW: "warning",
    CONFIRMED: "success",
    VOIDED: "neutral",
}

export const VIEW_LABEL: Record<SettlementView, string> = {
    pending: "待处理",
    prepared_by_me: "我经办",
    review_by_me: "我复核",
    confirmed: "已确认",
}

export const DIFF_TYPE_LABEL: Record<DifferenceType, string> = {
    MISSING_ORDER: "漏单",
    DUPLICATE: "重复",
    AMOUNT: "金额差异",
    REFUND: "退款差异",
    STATUS: "状态差异",
}

export const DIFF_STATUS_LABEL: Record<DifferenceStatus, string> = {
    PENDING: "待处理",
    SUPPLIER_ACCEPTED: "供应商认可",
    ERP_ACCEPTED: "ERP 认可",
    COMPENSATED: "已补偿",
    CLOSED: "关闭",
}

export const RESOLUTION_LABEL: Record<DifferenceResolution, string> = {
    SUPPLIER_ACCEPTED: "供应商认可",
    ERP_ACCEPTED: "ERP 认可",
    COMPENSATED: "已补偿",
    CLOSED_NO_ADJUSTMENT: "关闭（无需调整）",
}

export const SECTION_LABEL: Record<SettlementSection, string> = {
    overview: "概览",
    items: "结算明细",
    differences: "差异处理",
    review: "复核记录",
    payable: "应付与票款",
    audit: "审计",
}

export const SECTIONS: SettlementSection[] = [
    "overview",
    "items",
    "differences",
    "review",
    "payable",
    "audit",
]

/** 审计动作中文映射（动作码只在代码与审计数据结构中使用） */
export const AUDIT_ACTION_LABEL: Record<string, string> = {
    CREATE_DRAFT: "创建结算草稿",
    REFRESH_TRIAL: "刷新试算",
    RESOLVE_DIFFERENCE: "登记差异结论",
    APPEND_EVIDENCE: "追加采购证据",
    SUBMIT_REVIEW: "提交复核",
    CONFIRM: "确认结算",
    REJECT: "驳回复核",
}

/** 复核/结论原因码中文映射（原因码原值不上屏） */
export const REASON_CODE_LABEL: Record<string, string> = {
    NEEDS_MORE_EVIDENCE: "证据不足",
    AMOUNT_MISMATCH: "金额仍不一致",
    OTHER: "其他",
    BILL_ALIGNED: "账单已对齐",
    ACCEPT_BILL: "接受供应商账单",
    NO_BUSINESS_IMPACT: "无需业务调整",
    COMPENSATED_ELSEWHERE: "已另行补偿",
}
