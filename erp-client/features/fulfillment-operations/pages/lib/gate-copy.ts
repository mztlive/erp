/**
 * W09 先款条件徽章的文案口径。
 * 仓发与其余作业共用判定，仅措辞不同；面向一线作业角色整体口语化。
 */

import type { PrepaymentGateCopy } from "@/components/business/domain"

const SHIP_GATE_COPY: PrepaymentGateCopy = {
    title: "发货条件",
    description: "只认已经到账并核销过的货款，付款申请和附件不算。",
    allowedBadge: "可以发货",
    blockedBadge: "暂时不能发货",
    amountTerm: "至少要付",
    ratioTerm: "至少要付比例",
    allocatedTerm: "已经付了",
    gapTerm: "还差",
    updatedTerm: "算到什么时候",
    allowedTitle: "货款已到，可以发货",
    blockedTitle: "先款未到，暂时不能发货",
    allowedBody: "货款已经够了，这一单可以继续。",
    blockedBody: "差额补齐之前，仓发单据暂时不能确认发货。",
}

const RECEIVE_GATE_COPY: PrepaymentGateCopy = {
    title: "先款条件",
    description: "只认已经到账并核销过的货款，付款申请和附件不算。",
    allowedBadge: "可以收货",
    blockedBadge: "暂时不能收货",
    amountTerm: "至少要付",
    ratioTerm: "至少要付比例",
    allocatedTerm: "已经付了",
    gapTerm: "还差",
    updatedTerm: "算到什么时候",
    allowedTitle: "货款已到，可以收货",
    blockedTitle: "先款未到，暂时不能收货",
    allowedBody: "货款已经够了，这一单可以继续。",
    blockedBody:
        "差额补齐之前，入库、直发、电子交付和服务都确认不了。",
}

export function prepaymentGateCopy(
    isWarehouseShip: boolean,
): PrepaymentGateCopy {
    return isWarehouseShip ? SHIP_GATE_COPY : RECEIVE_GATE_COPY
}

export function paymentRegistrationHref(
    purchaseOrderId: string | undefined,
    currentUrl: string,
): string {
    return `/finance/supplier-accounts?from=W09&purchaseOrderId=${purchaseOrderId ?? ""}&returnTo=${encodeURIComponent(currentUrl)}`
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
    return `/sales/orders/${salesOrderId}?section=acceptance&from=W09&returnTo=${encodeURIComponent(currentUrl)}`
}

export function salesOrderHref(
    salesOrderId: string,
    currentUrl: string,
): string {
    return `/sales/orders/${salesOrderId}?from=W09&returnTo=${encodeURIComponent(currentUrl)}`
}
