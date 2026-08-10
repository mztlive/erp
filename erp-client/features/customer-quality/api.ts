/**
 * W15 客户经营质量 — 真实 HTTP。
 * 主视图依赖 E3 查询投影：`GET /admin/customer-quality`（含 as_of）。
 * 期间策略：`GET /admin/customer-quality/period-policy`。
 * 导出：`POST /admin/customer-quality/exports`。
 * 接口未落地时 ApiError 上抛，前端不造数、不用客户端时钟冒充 as_of。
 */

import { apiGet, apiPost } from "@/lib/api"

import type {
    CustomerQualityExportJob,
    CustomerQualityPeriodPolicy,
    CustomerQualityQuery,
    CustomerQualityScenario,
    CustomerQualityView,
} from "@/features/customer-quality/types"

export type PeriodPolicyInput = {
    scenario?: CustomerQualityScenario
}

/** 后端投影响应（E3 契约目标形状；字段 snake_case 与 api-contract 一致）。 */
type CustomerQualityViewDto = CustomerQualityView & {
    as_of?: string
    projected_at?: string
}

type PeriodPolicyDto = CustomerQualityPeriodPolicy & {
    has_default?: boolean
    period_basis?: string
    customer_quality_period_policy_id?: string
    customer_quality_period_policy_version?: number
    selection_source?: string
}

type ExportJobDto = {
    job_id?: string
    jobId?: string
    id?: string
    status?: CustomerQualityExportJob["status"]
    total?: number
    completed?: number
    filter_summary?: string
    period?: { from: string; to: string }
    permission_version?: string
    projection_watermark?: string
    amount_basis_note?: string
    download_label?: string
    expires_at?: string
}

function queryToParams(query: CustomerQualityQuery): Record<string, unknown> {
    return {
        from: query.from,
        to: query.to,
        period_basis: query.periodBasis,
        period_selection_source: query.periodSelectionSource,
        customer_quality_period_policy_id: query.customerQualityPeriodPolicyId,
        customer_quality_period_policy_version:
            query.customerQualityPeriodPolicyVersion,
        scope_id: query.scopeId,
        funds_review: query.fundsReview,
        business_type: query.businessType,
        benefit_scenario: query.benefitScenario,
        scale_tag: query.scaleTag,
        profit_tag: query.profitTag,
        risk_tag: query.riskTag,
        q: query.q,
        sort: query.sort,
        page: query.page,
        page_size: query.pageSize,
        chart_dimension: query.chartDimension,
        chart_code: query.chartCode,
        customer_id: query.customerId,
    }
}

function adaptPeriodPolicy(dto: PeriodPolicyDto): CustomerQualityPeriodPolicy {
    return {
        hasDefault: dto.hasDefault ?? dto.has_default ?? false,
        from: dto.from,
        to: dto.to,
        periodBasis: dto.periodBasis ?? dto.period_basis,
        timezone: dto.timezone ?? "Asia/Shanghai",
        customerQualityPeriodPolicyId:
            dto.customerQualityPeriodPolicyId ??
            dto.customer_quality_period_policy_id,
        customerQualityPeriodPolicyVersion:
            dto.customerQualityPeriodPolicyVersion ??
            dto.customer_quality_period_policy_version,
        selectionSource: (dto.selectionSource ??
            dto.selection_source) as CustomerQualityPeriodPolicy["selectionSource"],
        presets: dto.presets,
    }
}

/**
 * 适配 E3 投影视图：若后端用 as_of 表示新鲜度，写入 freshness.projectedAt。
 * 禁止用客户端时间填充。
 */
function adaptView(dto: CustomerQualityViewDto): CustomerQualityView {
    const asOf = dto.as_of ?? dto.projected_at
    if (asOf && dto.freshness) {
        return {
            ...dto,
            freshness: {
                ...dto.freshness,
                projectedAt: dto.freshness.projectedAt || asOf,
                sourceWatermark: dto.freshness.sourceWatermark || asOf,
            },
        }
    }
    return dto
}

export async function fetchCustomerQualityPeriodPolicy(
    input?: PeriodPolicyInput,
): Promise<CustomerQualityPeriodPolicy> {
    const dto = await apiGet<PeriodPolicyDto>(
        "/admin/customer-quality/period-policy",
        input?.scenario ? { scenario: input.scenario } : undefined,
    )
    return adaptPeriodPolicy(dto)
}

export async function fetchCustomerQuality(
    query: CustomerQualityQuery,
): Promise<CustomerQualityView> {
    const dto = await apiGet<CustomerQualityViewDto>(
        "/admin/customer-quality",
        queryToParams(query),
    )
    return adaptView(dto)
}

export async function startCustomerQualityExport(input: {
    query: CustomerQualityQuery
    filterSummary: string
    projectionWatermark: string
    permissionVersion: string
    rowCount: number
}): Promise<CustomerQualityExportJob> {
    const dto = await apiPost<ExportJobDto>("/admin/customer-quality/exports", {
        from: input.query.from,
        to: input.query.to,
        period_basis: input.query.periodBasis,
        scope_id: input.query.scopeId,
        funds_review: input.query.fundsReview,
        business_type: input.query.businessType,
        filter_summary: input.filterSummary,
        projection_watermark: input.projectionWatermark,
        permission_version: input.permissionVersion,
        row_count: input.rowCount,
    })

    return {
        jobId: dto.jobId ?? dto.job_id ?? dto.id ?? "",
        status: dto.status ?? "queued",
        total: dto.total ?? input.rowCount,
        completed: dto.completed ?? 0,
        filterSummary: dto.filter_summary ?? input.filterSummary,
        period: dto.period ?? { from: input.query.from, to: input.query.to },
        permissionVersion: dto.permission_version ?? input.permissionVersion,
        projectionWatermark:
            dto.projection_watermark ?? input.projectionWatermark,
        amountBasisNote:
            dto.amount_basis_note ??
            "成交金额为含税，实际盈亏为不含税；卡券收入不进入实际盈亏列；缺失成本不写作 0。",
        downloadLabel: dto.download_label,
        expiresAt: dto.expires_at,
    }
}
