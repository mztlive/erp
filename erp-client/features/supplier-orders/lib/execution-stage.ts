/** 后台真实采购执行阶段的业务文案，不从付款或根单状态推导确认方式。 */
export const SUPPLIER_EXECUTION_STAGE_LABEL: Readonly<Record<string, string>> =
    {
        QUEUED: "等待接口处理",
        AWAITING_SETUP: "等待补充配送资料",
        PREPARE_ADDRESS: "正在核验配送地址",
        ADDRESS_UNKNOWN: "地址提交结果待查询",
        PLAN_DELIVERY: "正在查询配送方案",
        CREATE: "正在向供应商下单",
        CREATION_UNKNOWN: "下单结果待查询",
        CONFIRM_PAYMENT: "正在确认供应商履约接口",
        PAYMENT_UNKNOWN: "供应商履约确认结果待查询",
        DISCOVER_ORDERS: "正在核验供应商子单",
        CONFIRM_ORDER: "正在确认供应商订单",
        CONFIRMATION_UNKNOWN: "订单确认结果待核实",
        OBSERVE: "订单已提交，持续跟踪中",
        STOPPED: "接口执行已暂停",
    }
