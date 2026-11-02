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
                label: "销售询价",
                href: "/sales/inquiries",
                permission: "sales_inquiry:list",
            },
            {
                label: "销售订单",
                href: "/sales/orders",
                permission: "sales_order:list",
            },
        ],
    },
    {
        id: "WB-03",
        name: "采购工作台",
        view: "procurement",
        description: "询报价、供给分配与采购执行",
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
        description: "收付款分配与已批准付款执行",
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
] as const
export type RoleWorkbenchId = (typeof ROLE_WORKBENCHES)[number]["id"]
export function roleWorkbench(id: string) {
    return ROLE_WORKBENCHES.find((item) => item.id === id)
}
