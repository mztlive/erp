import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    buildSelfHref,
    isActionableFocusTask,
    isOpenProcurementRejection,
    navItemsFor,
    rejectionAllowsResubmit,
    rejectionAllowsVoid,
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
        pageMode: string | null
        fromWorkspace: string | null
        returnTo: string | null
    },
) {
    const { section, pageMode, fromWorkspace, returnTo } = input

    const canAccept =
        order.nature === "physical_service" &&
        order.allowedActions.includes("REGISTER_ACCEPTANCE")
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
    const acceptanceExpanded = section === "acceptance"
    const showGoodsApproval =
        order.nature === "physical_service" && Boolean(order.approval)
    const showVoucherApproval =
        order.nature === "card_voucher" && Boolean(order.approval)
    const showApproval = showGoodsApproval || showVoucherApproval
    const canResubmit = rejectionAllowsResubmit(order)
    const canVoid = rejectionAllowsVoid(order)
    const canRequestLowMargin = Boolean(
        order.procurementRejection?.allowedActions.includes(
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
        ),
    )
    const showEditor = shouldOpenSalesOrderEditor({
        order,
        mode: pageMode,
        canResubmit,
    })

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

    const openRejection = isOpenProcurementRejection(order)
    const hasPrimaryTaskAction =
        Boolean(
            order.nature !== "physical_service" && openRejection && canResubmit,
        ) ||
        Boolean(
            actionableFocusTask &&
            !(
                order.nature !== "physical_service" &&
                openRejection &&
                navSection === "overview"
            ),
        )
    const bannerJump =
        Boolean(focusTask) &&
        !hasPrimaryTaskAction &&
        ((focusTask?.id === "versions" && navSection !== "versions") ||
            (focusTask?.id === "procurement-rejection" &&
                navSection !== "overview") ||
            (focusTask?.id === "acceptance" && !acceptanceExpanded))

    return {
        canAccept,
        canStartChange,
        changeBlocker,
        isCard,
        navSection,
        acceptanceExpanded,
        showApproval,
        canResubmit,
        canVoid,
        canRequestLowMargin,
        showEditor,
        focusTask,
        actionableFocusTask,
        returnSection,
        selfReturn,
        visibleNav,
        openRejection,
        hasPrimaryTaskAction,
        bannerJump,
    }
}
