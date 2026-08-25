import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    buildSelfHref,
    canRegisterCustomerAcceptance,
    isActionableFocusTask,
    navItemsFor,
    resolveFocusTask,
    resolveNavSection,
    shouldOpenSalesOrderEditor,
} from "@/features/sales-orders/lib/sales-order-detail-model"

/**
 * 详情页展示状态的一次性纯推导（无副作用）。
 * 必须在 `order` 已确认存在后调用；页面早退分支（加载中/失败/不存在）不经过这里。
 */
export function deriveSalesOrderDetailState(
    order: SalesOrderDetailView,
    input: {
        section?: string
        fromWorkspace: string | null
        returnTo: string | null
        hasEligibleAcceptance: boolean
    },
) {
    const { section, fromWorkspace, returnTo, hasEligibleAcceptance } = input

    const canAccept = canRegisterCustomerAcceptance(
        order,
        hasEligibleAcceptance,
    )
    const canStartChange =
        order.allowedActions.includes("START_SALES_CHANGE") ?? false
    const changeBlocker = order.actionBlockers.find(
        (b) => b.action === "START_SALES_CHANGE",
    )

    const isCard = order.nature === "card_voucher"
    const navSection = resolveNavSection(section, {
        from: fromWorkspace,
        isCard,
    })
    const showGoodsApproval =
        order.nature === "physical_service" && Boolean(order.approval)
    const showVoucherApproval =
        order.nature === "card_voucher" && Boolean(order.approval)
    const showApproval = showGoodsApproval || showVoucherApproval
    const showEditor = shouldOpenSalesOrderEditor(order)

    const focusTask = resolveFocusTask(order, Boolean(canAccept))
    const actionableFocusTask = isActionableFocusTask(focusTask)
        ? focusTask
        : null
    const returnSection =
        section && section !== "overview" ? section : navSection
    const selfReturn = encodeURIComponent(
        buildSelfHref(order.id, returnSection, {
            returnTo,
            from: fromWorkspace,
        }),
    )
    const visibleNav = navItemsFor(order).filter((item) => item.show)

    const hasPrimaryTaskAction = Boolean(actionableFocusTask)
    const bannerJump =
        Boolean(focusTask) &&
        !hasPrimaryTaskAction &&
        focusTask?.id === "versions" &&
        navSection !== "versions"

    return {
        canAccept,
        canStartChange,
        changeBlocker,
        isCard,
        navSection,
        showApproval,
        showEditor,
        focusTask,
        actionableFocusTask,
        returnSection,
        selfReturn,
        visibleNav,
        hasPrimaryTaskAction,
        bannerJump,
    }
}
