/**
 * W22 商品发布 · 枚举中文映射与展示色调（用户可见文案，已过术语表）。
 * 从 types.ts 拆出以控制文件体积；types.ts 原样再导出。
 */

import type { SafetyPauseCause } from "@/features/product-publications/lib/safety-pause-types"
import type {
    DeliveryStatus,
    PublicationMediaItem,
    PublicationStatus,
    SaleStatus,
} from "@/features/product-publications/types"

export const PUBLICATION_STATUS_LABEL: Record<PublicationStatus, string> = {
    DRAFT: "草稿",
    PENDING_PUBLISH: "待发布",
    MALL_LIVE: "商城已生效",
    PAUSED: "已暂停",
    SAFETY_PAUSED: "安全暂停",
    INVALID: "已失效",
}

export const PUBLICATION_STATUS_TONE: Record<
    PublicationStatus,
    "success" | "warning" | "destructive" | "info" | "neutral"
> = {
    DRAFT: "neutral",
    PENDING_PUBLISH: "info",
    MALL_LIVE: "success",
    PAUSED: "warning",
    SAFETY_PAUSED: "destructive",
    INVALID: "neutral",
}

export const DELIVERY_STATUS_LABEL: Record<DeliveryStatus, string> = {
    PENDING_SEND: "待发送",
    SENDING: "发送中",
    RETRYING: "重试中",
    ACKED: "已确认",
    FAILED: "失败",
    HANDOFF: "转人工",
}

export const DELIVERY_STATUS_TONE: Record<
    DeliveryStatus,
    "success" | "warning" | "destructive" | "info" | "neutral"
> = {
    PENDING_SEND: "info",
    SENDING: "info",
    RETRYING: "warning",
    ACKED: "success",
    FAILED: "destructive",
    HANDOFF: "warning",
}

export const SALE_STATUS_LABEL: Record<SaleStatus, string> = {
    ON_SALE: "上架",
    OFF_SALE: "下架",
    PAUSED: "暂停下单",
}

export const SAFETY_PAUSE_CAUSE_LABEL: Record<SafetyPauseCause, string> = {
    SUPPLIER_STOPPED: "供应商停供",
    ZERO_INVENTORY: "零库存",
    SUPPLY_UNAVAILABLE: "明确不可供",
    AVAILABILITY_STALE: "可供数据过期",
    COST_CHANGE_UNCONFIRMED: "成本变化未确认",
    CRITICAL_SUPPLY_CHANGE_UNCONFIRMED: "关键供给变化未确认",
}

export const MEDIA_ROLE_LABEL: Record<
    PublicationMediaItem["mediaRole"],
    string
> = {
    MAIN: "主图",
    CAROUSEL: "轮播图",
    DETAIL: "详情图",
}

export const MEDIA_SCAN_STATUS_LABEL: Record<
    PublicationMediaItem["securityScanStatus"],
    string
> = {
    PASSED: "已通过",
    PENDING: "检查中",
    FAILED: "未通过",
}

/** 安全暂停后续任务类型中文名（禁止枚举原值上屏）。 */
export const WORK_ITEM_TYPE_LABEL: Record<string, string> = {
    BUSINESS_EXCEPTION: "业务异常",
}
