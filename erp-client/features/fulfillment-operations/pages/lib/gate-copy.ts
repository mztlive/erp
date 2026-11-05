/**
 * 履约作业面的采购付款提示文案。
 */

import type { PrepaymentGateCopy } from "@/components/business/domain"

/** 付款信息仅作提醒，不作为履约确认资格。 */
export function prepaymentGateCopy(): PrepaymentGateCopy {
    return {
        title: "采购付款",
        description: "付款申请和审批通过不代表已支付；付款不阻止履约确认。",
        allowedBadge: "预付已满足",
        blockedBadge: "预付待完成",
        amountTerm: "约定预付",
        ratioTerm: "约定预付比例",
        allocatedTerm: "有效已付",
        gapTerm: "还差",
        updatedTerm: "更新时间",
        allowedTitle: "预付条件已满足",
        blockedTitle: "预付待完成，不影响确认实际履约",
        allowedBody: "请按实际发货或交付情况确认履约。",
        blockedBody: "供应商支持先发货时，可先确认实际履约，再按约定办理付款。",
    }
}

export function paymentRegistrationHref(
    purchaseOrderId: string | undefined,
    currentUrl: string,
): string {
    return `/procurement/payment-requests?create=1&purchaseOrderId=${purchaseOrderId ?? ""}&returnTo=${encodeURIComponent(currentUrl)}`
}

/**
 * 履约成功后跳到销售单客户验收。CustomerAcceptance 为 NO_APPROVAL，
 * 只交接业务登记，不打开审批流程或决定入口。
 *
 * @param salesOrderId 销售单 ID。
 * @param currentUrl 当前履约页 URL，用于返回。
 * @returns 销售单验收分区路径。
 */
export function acceptanceHref(
    salesOrderId: string,
    currentUrl: string,
): string {
    return `/sales/orders/${salesOrderId}?section=acceptance&from=W01&returnTo=${encodeURIComponent(currentUrl)}`
}

export function salesOrderHref(
    salesOrderId: string,
    currentUrl: string,
): string {
    return `/sales/orders/${salesOrderId}?from=W01&returnTo=${encodeURIComponent(currentUrl)}`
}
