/**
 * W28 卡券消费台账与经营分析 — 真实 HTTP。
 * - 主投影：`GET /admin/card-business-analytics`（E3，含 as_of / 覆盖率三分法）
 * - 日期口径：`GET /admin/card-business-analytics/date-basis`
 * - 导出：`POST /admin/card-business-analytics/exports`
 * 卡实例台账辅助：`GET /admin/card-instances`（D28，非经营汇总）。
 * 禁止前端重算覆盖率/利润；禁止客户端时钟冒充 as_of。
 */

import { apiGet, apiPost } from "@/lib/api"

import type {
    CardBusinessAnalyticsQuery,
    CardBusinessAnalyticsView,
    CardBusinessExportJob,
    DateBasisConfig,
} from "../types"

export type DateBasisConfigQuery = {
    /** QA：basisConfig=missing；真后端无对应参数时忽略 */
    scenario?: "default" | "missing"
}

type DateBasisDto = DateBasisConfig & {
    configured_date_basis?: DateBasisConfig["configuredDateBasis"]
    configuration_version?: string
    allowed_date_bases?: DateBasisConfig["allowedDateBases"]
}

type CardBusinessViewDto = CardBusinessAnalyticsView & {
    as_of?: string
    projected_at?: string
}

type ExportJobDto = {
    job_id?: string
    jobId?: string
    id?: string
    status?: CardBusinessExportJob["status"]
    total?: number
    completed?: number
    created_at?: string
    createdAt?: string
    download_label?: string
    watermark?: CardBusinessExportJob["watermark"]
}

function queryToParams(
    query: CardBusinessAnalyticsQuery,
): Record<string, unknown> {
    return {
        from: query.from,
        to: query.to,
        date_basis: query.dateBasis,
        dimension: query.dimension,
        customer_id: query.customerId,
        sales_order_id: query.salesOrderId,
        voucher_category_id: query.voucherCategoryId,
        cost_basis: query.costBasis?.join(","),
        expiry_state: query.expiryState,
        coverage: query.coverage,
        compare: query.compare,
        sort: query.sort,
        page: query.page,
        page_size: query.pageSize,
    }
}

function adaptDateBasis(dto: DateBasisDto): DateBasisConfig {
    return {
        configuredDateBasis:
            dto.configuredDateBasis ?? dto.configured_date_basis,
        allowedDateBases: dto.allowedDateBases ?? dto.allowed_date_bases ?? [],
        configurationVersion:
            dto.configurationVersion ?? dto.configuration_version ?? "",
    }
}

function adaptView(dto: CardBusinessViewDto): CardBusinessAnalyticsView {
    const asOf = dto.as_of ?? dto.projected_at
    if (asOf && dto.freshness) {
        return {
            ...dto,
            freshness: {
                ...dto.freshness,
                projectionUpdatedAt: dto.freshness.projectionUpdatedAt || asOf,
                sourceFactWatermark: dto.freshness.sourceFactWatermark || asOf,
                consumedOutboxWatermark:
                    dto.freshness.consumedOutboxWatermark || asOf,
            },
        }
    }
    return dto
}

export async function fetchDateBasisConfig(
    query: DateBasisConfigQuery = {},
): Promise<DateBasisConfig> {
    const dto = await apiGet<DateBasisDto>(
        "/admin/card-business-analytics/date-basis",
        query.scenario ? { scenario: query.scenario } : undefined,
    )
    return adaptDateBasis(dto)
}

export async function fetchCardBusinessAnalytics(
    query: CardBusinessAnalyticsQuery,
): Promise<CardBusinessAnalyticsView> {
    const dto = await apiGet<CardBusinessViewDto>(
        "/admin/card-business-analytics",
        queryToParams(query),
    )
    return adaptView(dto)
}

export async function startCardBusinessExport(input: {
    query: CardBusinessAnalyticsQuery
    view: Pick<
        CardBusinessAnalyticsView,
        | "period"
        | "scope"
        | "freshness"
        | "coverage"
        | "filterSummary"
        | "wechatExcludedNote"
        | "fieldPermissions"
        | "rows"
    >
}): Promise<CardBusinessExportJob> {
    if (!input.view.fieldPermissions.canExport) {
        throw {
            kind: "Validation" as const,
            message: "当前权限不允许导出",
            status: 403,
        }
    }
    if (!input.query.dateBasis || !input.query.from || !input.query.to) {
        throw {
            kind: "Validation" as const,
            message: "from/to/dateBasis 未完整，已阻断导出",
            status: 400,
        }
    }

    const dto = await apiPost<ExportJobDto>(
        "/admin/card-business-analytics/exports",
        {
            from: input.view.period.from,
            to: input.view.period.to,
            date_basis: input.view.period.dateBasis,
            filter_summary: input.view.filterSummary,
            coverage_rate: input.view.coverage.rate,
            projection_updated_at: input.view.freshness.projectionUpdatedAt,
            consumed_outbox_watermark:
                input.view.freshness.consumedOutboxWatermark,
            row_count: input.view.rows.total,
        },
    )

    return {
        jobId: dto.jobId ?? dto.job_id ?? dto.id ?? "",
        status: dto.status ?? "queued",
        total: dto.total ?? input.view.rows.total,
        completed: dto.completed ?? 0,
        createdAt: dto.createdAt ?? dto.created_at ?? "",
        downloadLabel: dto.download_label,
        watermark: dto.watermark ?? {
            periodFrom: input.view.period.from,
            periodTo: input.view.period.to,
            dateBasis: input.view.period.dateBasis,
            filterSummary: input.view.filterSummary,
            coverageRate: input.view.coverage.rate,
            projectionUpdatedAt: input.view.freshness.projectionUpdatedAt,
            consumedOutboxWatermark:
                input.view.freshness.consumedOutboxWatermark,
            balanceSnapshotAt: input.view.freshness.balanceSnapshotAt,
            lagSeconds: input.view.freshness.lagSeconds,
            permissionVersion: input.view.scope.permissionVersion,
            taxDisclaimer:
                "销售/面值/消费/余额为含税；成本/毛差/经营贡献为不含税。无可用成本不按零成本计入利润。",
            wechatExcludedNote: input.view.wechatExcludedNote,
            rowCount: input.view.rows.total,
        },
    }
}
