import type { RoleWorkbenchId } from "@/lib/role-workbenches"

/** 只配置岗位展示文案；统计来源、责任范围和处理资格沿用既有业务接口。 */
export const WORKBENCH_PRESENTATION = {
    "WB-01": {
        prefix: "sales",
        description: "跟进询价、推进订单、完成客户验收",
        timingTitle: "客户跟进任务时限",
        stagesTitle: "销售单阶段分布",
        scopeLabel: "销售单授权范围",
    },
    "WB-03": {
        prefix: "procurement",
        description: "跟进询报价、落实采购、推进付款与执行",
        timingTitle: "采购任务时限",
        stagesTitle: "采购执行阶段",
        scopeLabel: "采购单授权范围",
    },
    "WB-04": {
        prefix: "cashier",
        description: "执行已批准付款、处理退货款、跟进资金分配",
        timingTitle: "付款执行时限",
        stagesTitle: "待分配资金",
        scopeLabel: "资金业务授权范围",
    },
} as const satisfies Record<
    RoleWorkbenchId,
    {
        prefix: string
        description: string
        timingTitle: string
        stagesTitle: string
        scopeLabel: string
    }
>
