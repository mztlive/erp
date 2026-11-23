/** 稳定岗位编号；入口以服务端账号资料中的显式映射为准。 */
export const ROLE_WORKBENCHES = [
    {
        id: "WB-01",
        name: "销售工作台",
        view: "sales",
        description: "询价跟进、订单推进与客户验收",
        tasks: ["SALES_INQUIRY_PROCESSING", "CUSTOMER_ACCEPTANCE_REGISTRATION"],
        operations: [
            {
                label: "查看销售单",
                href: "/sales/orders",
                permission: "sales_order:list",
            },
            {
                label: "查看销售询价",
                href: "/sales/inquiries",
                permission: "sales_inquiry:list",
            },
        ],
    },
    {
        id: "WB-02",
        name: "履约仓储工作台",
        view: "fulfillment",
        description: "收货入库、仓发直发与交付执行",
        tasks: ["FULFILLMENT_OPERATION", "CUSTOMER_ACCEPTANCE_REGISTRATION"],
        operations: [
            {
                label: "收货与发货",
                href: "/workspace/tasks?family=fulfillment&type=FULFILLMENT_OPERATION",
                permission: "work_item:list",
            },
            {
                label: "库存台账",
                href: "/inventory",
                permission: "stock_balance:list",
            },
            {
                label: "采购订单",
                href: "/procurement/orders",
                permission: "purchase_order:list",
            },
            {
                label: "销售单",
                href: "/sales/orders",
                permission: "sales_order:list",
            },
        ],
    },
    {
        id: "WB-03",
        name: "采购工作台",
        view: "procurement",
        description: "询价比价、货源安排与采购跟进",
        tasks: [
            "PROCUREMENT_ORDER_CREATION",
            "PURCHASE_PAYMENT_ARRANGEMENT",
            "PURCHASE_SOURCE_UPDATE",
        ],
        operations: [
            {
                label: "采购询价",
                href: "/procurement/inquiries",
                permission: "procurement_inquiry:list",
            },
            {
                label: "采购订单",
                href: "/procurement/orders",
                permission: "purchase_order:list",
            },
        ],
    },
    {
        id: "WB-04",
        name: "出纳工作台",
        view: "finance",
        description: "客户回款核销与供应商付款办理",
        tasks: ["SUPPLIER_PAYMENT_EXECUTION", "PURCHASE_RETURN_REFUND"],
        operations: [
            {
                label: "客户资金往来",
                href: "/finance/customer-accounts?view=receipt",
                permission: "customer_receipt:list",
            },
            {
                label: "供应商资金往来",
                href: "/finance/supplier-accounts?view=payment",
                permission: "supplier_payment:list",
            },
        ],
    },
    {
        id: "WB-05",
        name: "经营管理工作台",
        view: "management",
        description: "实际经营盈亏、订单推进与重点审批",
        tasks: [
            "DOCUMENT_APPROVAL",
            "BUSINESS_EXCEPTION",
            "INTEGRATION_RESULT_UNKNOWN",
        ],
        operations: [
            {
                label: "实际经营盈亏",
                href: "/analytics/profit-loss",
                permission: "cost_entry:list",
            },
            {
                label: "客户经营质量",
                href: "/analytics/customer-quality",
                permission: "customer:list",
            },
            {
                label: "查看销售单",
                href: "/sales/orders",
                permission: "sales_order:list",
            },
            {
                label: "审批跟踪",
                href: "/workspace/approvals",
                permission: "approval_instance:read",
            },
        ],
    },
] as const
export type RoleWorkbenchId = (typeof ROLE_WORKBENCHES)[number]["id"]
export function roleWorkbench(id: string) {
    return ROLE_WORKBENCHES.find((item) => item.id === id)
}
