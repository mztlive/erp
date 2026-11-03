import {
    displayActorName,
    displayReadableName,
} from "@/features/approval-workflow/display"
import { compareDecimal } from "@/lib/fixed-decimal"
import type {
    BackendSalesOrderCurrentWork,
    SalesOrderDetailView,
} from "../api/contracts"
import type { AcceptanceOrderProgress } from "./acceptance-model"
import { getProcurementAttention } from "./procurement-attention"
import {
    isSalesOrderApprovalInProgress,
    type NavSectionId,
} from "./sales-order-detail-model"

export type ProcessState = "done" | "active" | "waiting" | "unknown" | "stopped"
export type WorkGroup = {
    id: string
    label: string
    owner: string
    count: number
    tasks: BackendSalesOrderCurrentWork[]
}
export type ProcessStep = {
    id: string
    title: string
    state: ProcessState
    description: string
    owner?: string
    currentNode?: string
    groups: WorkGroup[]
    section: NavSectionId
    money?: string
}
export type ProcessLane = {
    id: "delivery" | "collection" | "invoice"
    title: string
    steps: ProcessStep[]
}

/** 同一环节与处理人合并；数量使用服务端去重后的摘要，不读取关联单据身份。 */
function workGroups(tasks: BackendSalesOrderCurrentWork[]): WorkGroup[] {
    const groups = new Map<string, BackendSalesOrderCurrentWork[]>()
    for (const task of tasks) {
        const key = JSON.stringify([
            task.kind,
            task.current_node,
            task.label,
            task.role_label,
            task.owner_user_id,
        ])
        groups.set(key, [...(groups.get(key) ?? []), task])
    }
    return Array.from(groups, ([id, items]) => ({
        id,
        label: items[0].label,
        owner: `${items[0].role_label} · ${displayActorName(items[0].owner_name) || (items[0].owner_user_id ? "姓名待补全" : "待分配")}`,
        count: items.reduce((count, item) => count + item.count, 0),
        tasks: items,
    }))
}

/** 各轨独立判断；没有读取到数据时不得用「待开始」或「已完成」替代未知。 */
export function salesOrderProcess(
    order: SalesOrderDetailView,
    delivery?: AcceptanceOrderProgress,
): { root: ProcessStep; lanes: ProcessLane[] } {
    const effective = order.currentRevisionNo != null
    const voided = order.primaryStatus.code === "voided"
    const closed = order.primaryStatus.code === "closed"
    const tasks = voided ? [] : (order.currentWork ?? [])
    const known = order.currentWork !== undefined
    const approvalTasks = tasks.filter((task) => task.section === "approval")
    const root: ProcessStep = {
        id: "sales",
        title: effective ? "销售审批" : "销售单",
        state: voided
            ? "stopped"
            : effective
              ? "done"
              : isSalesOrderApprovalInProgress(order)
                ? "active"
                : "waiting",
        description: voided
            ? "已作废"
            : effective
              ? "已通过"
              : order.primaryStatus.label,
        currentNode: effective
            ? undefined
            : displayReadableName(
                  order.approval?.instance?.currentNodeName,
                  order.approval?.instance?.currentNode,
              ),
        owner: effective
            ? undefined
            : [
                  ...new Set(
                      approvalTasks.map(
                          (task) =>
                              `${task.owner_org_name || "部门待确认"} · ${displayActorName(task.owner_name) || "处理人待确认"}`,
                      ),
                  ),
              ].join("、") ||
              displayActorName(order.approval?.instance?.currentAssigneeName) ||
              displayActorName(order.primaryStatus.ownerUserName) ||
              "处理人待确认",
        groups: workGroups(approvalTasks),
        section: "approval",
    }
    const step = (
        id: string,
        title: string,
        section: NavSectionId,
        relevant: BackendSalesOrderCurrentWork[],
        done: boolean,
        description: string,
        active = false,
    ): ProcessStep => ({
        id,
        title,
        section,
        state: done
            ? "done"
            : voided
              ? "stopped"
              : !effective
                ? "waiting"
                : relevant.length || active
                  ? "active"
                  : !known
                    ? "unknown"
                    : "waiting",
        description: done
            ? "已完成"
            : voided
              ? "已终止"
              : !effective
                ? "销售单生效后推进"
                : !known
                  ? "办理信息暂不可用"
                  : description,
        groups: done ? [] : workGroups(relevant),
    })
    const fulfilled = closed || order.closeEligibility.fulfillmentComplete
    const procurement = tasks.filter(
        (task) =>
            task.section === "fulfillment" &&
            task.kind !== "FULFILLMENT_OPERATION",
    )
    const shipping = tasks.filter(
        (task) =>
            task.section === "fulfillment" &&
            task.kind === "FULFILLMENT_OPERATION",
    )
    const acceptance = tasks.filter((task) => task.section === "acceptance")
    const attention = getProcurementAttention(order.related.procurementProgress)
    const delivered =
        fulfilled ||
        Boolean(
            delivery?.lines.length &&
            delivery.lines.every(
                (line) =>
                    compareDecimal(
                        line.deliveredQuantity,
                        line.requiredQuantity,
                        6,
                    ) >= 0,
            ),
        )
    const accepted =
        fulfilled ||
        Boolean(
            delivery?.lines.length &&
            delivery.lines.every((line) => line.stuckKind === "done"),
        )
    const supply = step(
        "procurement",
        "采购安排",
        "fulfillment",
        procurement,
        fulfilled ||
            (known &&
                effective &&
                !attention.needsAttention &&
                procurement.length === 0),
        attention.hasGap ? "供给未覆盖 · 采购责任待确认" : "查看供给安排",
        attention.needsAttention,
    )
    if (supply.groups.length)
        supply.title =
            new Set(supply.groups.map((group) => group.label)).size === 1
                ? supply.groups[0].label
                : "采购办理中"
    if (supply.state === "done") supply.description = "供给已安排"
    const dispatch = step(
        "dispatch",
        "安排交付",
        "fulfillment",
        shipping,
        delivered,
        delivery ? "待交付" : "交付进度待查询",
        Boolean(
            delivery?.lines.some(
                (line) => compareDecimal(line.deliveredQuantity, "0", 6) > 0,
            ),
        ),
    )
    const accept = step(
        "acceptance",
        "客户验收",
        "acceptance",
        acceptance,
        accepted,
        delivery?.pendingFactCount
            ? `${delivery.pendingFactCount} 批交付待验收`
            : delivery
              ? "待交付后验收"
              : "验收进度待查询",
        Boolean(delivery?.pendingFactCount),
    )
    if (effective && !voided && !fulfilled && !delivery) {
        if (!shipping.length) dispatch.state = "unknown"
        if (!acceptance.length) accept.state = "unknown"
    }
    if (accept.state === "active" && !accept.groups.length)
        accept.owner = `销售跟进 · ${displayActorName(order.ownerName) || "姓名待补全"}`
    const collectionDone = order.closeEligibility.receivableSettled
    const collect = step(
        "collection-followup",
        "跟进回款",
        "receipt",
        [],
        collectionDone,
        "按约定账期跟进客户付款",
        true,
    )
    if (effective && !voided && !collectionDone)
        collect.owner = displayActorName(order.ownerName) || "销售负责人待确认"
    const receipt = step(
        "receipt",
        "到账登记",
        "receipt",
        [],
        collectionDone,
        compareDecimal(order.receivedAmount, "0", 2) > 0
            ? "部分回款已登记"
            : "财务登记",
        compareDecimal(order.receivedAmount, "0", 2) > 0,
    )
    const invoiceDone = order.closeEligibility.invoiceComplete
    const invoiceTasks = tasks.filter((task) => task.section === "invoice")
    const executions = invoiceTasks.filter(
        (task) => task.kind === "SALES_INVOICE_EXECUTION",
    )
    const applications = invoiceTasks.filter(
        (task) => task.kind !== "SALES_INVOICE_EXECUTION",
    )
    const invoiceRequest = step(
        "invoice-request",
        "开票申请",
        "invoice",
        applications,
        invoiceDone,
        executions.length ? "已有申请待开票" : "暂无待办 · 查看开票申请",
    )
    if (
        effective &&
        !voided &&
        executions.length &&
        !applications.length &&
        !invoiceDone
    ) {
        invoiceRequest.state = "done"
        invoiceRequest.description = "已有申请通过审批"
    }
    // 多笔开票申请可并行，出现执行任务不能覆盖其他待审批申请。
    const invoice = step(
        "invoice-execution",
        "财务开票",
        "invoice",
        executions,
        invoiceDone,
        compareDecimal(order.invoicedAmount, "0", 2) > 0
            ? "已部分开票"
            : "待申请后办理",
    )
    return {
        root,
        lanes: [
            {
                id: "delivery",
                title:
                    order.nature === "card_voucher" ? "卡券履约" : "采购与交付",
                steps:
                    order.nature === "card_voucher"
                        ? [
                              step(
                                  "voucher",
                                  "卡券履约",
                                  "fulfillment",
                                  [],
                                  fulfilled,
                                  "按约定履约期限推进",
                                  true,
                              ),
                          ]
                        : [supply, dispatch, accept],
            },
            { id: "collection", title: "回款", steps: [collect, receipt] },
            { id: "invoice", title: "开票", steps: [invoiceRequest, invoice] },
        ],
    }
}
