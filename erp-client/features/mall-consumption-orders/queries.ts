/**
 * 兼容导出：外部模块（如 execution-projections 协同卡）仍从本路径导入。
 * 实际实现见 hooks/queries.ts。
 */
export {
    useConsumptionOrderDetailQuery,
    useConsumptionOrderExportMutation,
    useConsumptionOrderListQuery,
    useSalesOrderConsumptionSummaryQuery,
} from "./hooks/queries"
