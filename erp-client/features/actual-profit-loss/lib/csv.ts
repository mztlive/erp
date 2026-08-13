import type {
    ProfitLossCoverage,
    ProfitLossExportJob,
    ProfitLossView,
} from "@/features/actual-profit-loss/types"
import {
    COVERAGE_FILTER_LABEL,
    COVERAGE_STATE_UI,
} from "@/features/actual-profit-loss/types"

import { basisLabel } from "./url-state"

/** 客户端落盘 CSV：附带水印元数据（与后台任务一致；用户产物使用中文口径） */
export function buildProfitLossCsv(
    data: ProfitLossView,
    wm: ProfitLossExportJob["watermark"],
    coverage: ProfitLossCoverage,
): string {
    const quote = (v: string) => `"${v.replaceAll('"', '""')}"`
    const metaLines = [
        "# 业务口径=非卡券·不含税",
        `# 期间=${wm.periodFrom} ~ ${wm.periodTo}`,
        `# 归属口径=${basisLabel(wm.periodBasis)}`,
        `# 覆盖口径=${COVERAGE_FILTER_LABEL[coverage]}`,
        `# 范围=${wm.scopeLabel}`,
        `# 数据更新时间=${wm.projectedAt}`,
        `# 来源更新=${wm.sourceWatermark}`,
        `# 金额口径=不含税`,
        `# 业务类型=非卡券`,
        `# 行数=${wm.rowCount}`,
    ]
    const header =
        "销售单号,客户,不含税收入,实际采购成本,实际履约费用,成本冲减,实际盈亏,利润率,覆盖状态,缺口原因"
    const body = data.rows.items.map((r) =>
        [
            r.identityLabel,
            r.customerLabel ?? "",
            r.netSalesRevenue,
            r.actualProcurementCostNet ?? "",
            r.actualFulfillmentCostNet ?? "",
            r.reductionsNet ?? "",
            r.actualProfitLossNet ?? "",
            r.marginRate ?? r.marginUnavailableReason ?? "",
            COVERAGE_STATE_UI[r.coverageState].label,
            r.coverageBlockers.map((b) => b.message).join("|"),
        ]
            .map((c) => quote(String(c)))
            .join(","),
    )
    return [...metaLines, header, ...body].join("\n")
}
