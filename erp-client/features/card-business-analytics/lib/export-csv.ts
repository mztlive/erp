import type { CardBusinessAnalyticsView, CardBusinessExportJob } from "../types"
import { COST_BASIS_LABEL, DATE_BASIS_LABEL } from "../types"

/** 导出成功条的真实下载入口：按当前视图行生成 CSV（含口径/筛选/时间水印）。 */
export function downloadCardBusinessCsv(
    data: CardBusinessAnalyticsView,
    job: CardBusinessExportJob,
) {
    const wm = job.watermark
    const quote = (v: string) => `"${v.replaceAll('"', '""')}"`
    const metaLines = [
        "# 业务口径=卡券经营（销售/面值/消费/余额为含税；成本/毛差/经营贡献为不含税）",
        `# 期间=${wm.periodFrom} ~ ${wm.periodTo}`,
        `# 日期口径=${DATE_BASIS_LABEL[wm.dateBasis]}`,
        `# 筛选=${wm.filterSummary}`,
        `# 覆盖率=${wm.coverageRate ?? "—"}`,
        `# 数据更新时间=${wm.projectionUpdatedAt}`,
        `# 同步时间=${wm.consumedOutboxWatermark}`,
        `# 余额快照时间=${wm.balanceSnapshotAt ?? "—"}`,
        `# 延迟=${wm.lagSeconds} 秒`,
        `# 行数=${wm.rowCount}`,
        `# 微信排除=${wm.wechatExcludedNote}`,
    ]
    const header =
        "客户,销售单,卡券类目,卡实例引用,消费(含税),退款(含税),成本口径,成本(不含税),覆盖,未履约余额(含税)"
    const body = data.rows.items.map((r) =>
        [
            r.customerLabel,
            r.salesOrderNo ?? "",
            r.voucherCategoryLabel,
            r.cardInstanceRef ?? "",
            r.consumptionGross,
            r.refundGross,
            COST_BASIS_LABEL[r.costBasis],
            r.costNet ?? "",
            r.coverageStatus === "covered"
                ? "已覆盖"
                : r.coverageStatus === "partial"
                  ? "部分"
                  : "未覆盖",
            r.unfulfilledBalanceGross,
        ]
            .map((c) => quote(String(c)))
            .join(","),
    )
    const csv = [...metaLines, header, ...body].join("\n")
    const url = URL.createObjectURL(
        new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download =
        wm.periodFrom && wm.periodTo
            ? `卡券经营分析_${wm.periodFrom}_${wm.periodTo}.csv`
            : "卡券经营分析.csv"
    anchor.click()
    URL.revokeObjectURL(url)
}
