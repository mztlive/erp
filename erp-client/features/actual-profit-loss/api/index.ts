/**
 * W16 实际经营盈亏 — 真实 HTTP。
 * - 主投影视图：`GET /admin/actual-profit-loss`（E3，含 as_of）— 缺则 ApiError
 * - 期间口径配置：`GET /admin/actual-profit-loss/period-basis`
 * - 成本明细：`GET /admin/cost-entries/{id}`（D20，已落地）
 * - 导出：`POST /admin/actual-profit-loss/exports`
 * 禁止前端重算金额/覆盖率；禁止客户端时钟冒充 as_of。
 */

import { apiGet, apiPost } from "@/lib/api"

import type {
    CostEntryDetail,
    CostStage,
    ProfitLossExportJob,
    ProfitLossPeriodBasisConfig,
    ProfitLossQuery,
    ProfitLossView,
} from "@/features/actual-profit-loss/types"

export type PeriodBasisConfigQuery = {
    /** QA：basisConfig=missing；真后端无对应参数时忽略 */
    scenario?: "default" | "missing"
}

/** 后端成本事实 DTO（CostEntryView）。 */
type CostEntryDto = {
    id: string
    cost_type: string
    cost_stage: string
    cost_scope: string
    cost_basis?: string | null
    supplier_id?: string | null
    gross_amount: string
    net_amount: string
    tax_amount: string
    tax_inclusion: boolean
    input_tax_rate: string
    occurred_at: number
    source_fact_type: string
    source_document_id: string
    source_line_id: string
    source_version: string
    created_at: number
    allocations: Array<{
        id: string
        cost_entry_id: string
        sales_order_id?: string | null
        sales_order_line_id?: string | null
        allocated_gross_amount: string
        allocated_net_amount: string
        rounding_residual_flag: boolean
    }>
}

type PeriodBasisDto = ProfitLossPeriodBasisConfig & {
    configured_period_basis?: string
    configuration_version?: string
    allowed_period_bases?: ProfitLossPeriodBasisConfig["allowedPeriodBases"]
}

type ProfitLossViewDto = ProfitLossView & {
    as_of?: string
    projected_at?: string
}

type ExportJobDto = {
    job_id?: string
    jobId?: string
    id?: string
    status?: ProfitLossExportJob["status"]
    total?: number
    completed?: number
    created_at?: string
    createdAt?: string
    download_label?: string
    watermark?: ProfitLossExportJob["watermark"]
}

const COST_STAGE_LABEL: Record<string, string> = {
    EXPECTED: "预计",
    CONFIRMED: "确认",
    ACTUAL: "实际",
    REDUCTION: "冲减",
}

const COST_SCOPE_LABEL: Record<string, string> = {
    NON_VOUCHER_FULFILLMENT: "非卡券履约",
}

function unixToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

function mapCostEntry(dto: CostEntryDto): CostEntryDetail {
    const primary = dto.allocations[0]
    return {
        costEntryId: dto.id,
        costType: dto.cost_type,
        costTypeLabel: dto.cost_type,
        stage: dto.cost_stage as CostStage,
        stageLabel: COST_STAGE_LABEL[dto.cost_stage] ?? dto.cost_stage,
        costScope: dto.cost_scope as CostEntryDetail["costScope"],
        costScopeLabel: COST_SCOPE_LABEL[dto.cost_scope] ?? dto.cost_scope,
        supplierId: dto.supplier_id ?? undefined,
        amountGross: String(dto.gross_amount),
        taxRate: String(dto.input_tax_rate),
        taxAmount: String(dto.tax_amount),
        amountNet: String(dto.net_amount),
        occurredAt: unixToIso(dto.occurred_at),
        sourceType: dto.source_fact_type,
        sourceTypeLabel: dto.source_fact_type,
        sourceDocumentId: dto.source_document_id,
        sourceDocumentNo: dto.source_document_id,
        sourceLineId: dto.source_line_id || undefined,
        sourceVersion: dto.source_version,
        salesOrderId: primary?.sales_order_id ?? "",
        salesOrderNo: primary?.sales_order_id ?? "",
        salesOrderLineId: primary?.sales_order_line_id ?? undefined,
    }
}

function queryToParams(query: ProfitLossQuery): Record<string, unknown> {
    return {
        from: query.from,
        to: query.to,
        period_basis: query.periodBasis,
        scope_id: query.scopeId,
        coverage: query.coverage,
        customer_id: query.customerId,
        sales_order_id: query.salesOrderId,
        benefit_scenario: query.benefitScenario,
        fulfillment_modes: query.fulfillmentModes?.join(","),
        cost_types: query.costTypes?.join(","),
        dimension: query.dimension,
        q: query.q,
        sort: query.sort,
        page: query.page,
        page_size: query.pageSize,
    }
}

function adaptPeriodBasis(dto: PeriodBasisDto): ProfitLossPeriodBasisConfig {
    return {
        configuredPeriodBasis:
            dto.configuredPeriodBasis ?? dto.configured_period_basis,
        allowedPeriodBases:
            dto.allowedPeriodBases ?? dto.allowed_period_bases ?? [],
        configurationVersion:
            dto.configurationVersion ?? dto.configuration_version ?? "",
    }
}

function adaptProfitLossView(dto: ProfitLossViewDto): ProfitLossView {
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

export async function fetchPeriodBasisConfig(
    query: PeriodBasisConfigQuery = {},
): Promise<ProfitLossPeriodBasisConfig> {
    const dto = await apiGet<PeriodBasisDto>(
        "/admin/actual-profit-loss/period-basis",
        query.scenario ? { scenario: query.scenario } : undefined,
    )
    return adaptPeriodBasis(dto)
}

export async function fetchProfitLossView(
    query: ProfitLossQuery,
): Promise<ProfitLossView> {
    const dto = await apiGet<ProfitLossViewDto>(
        "/admin/actual-profit-loss",
        queryToParams(query),
    )
    return adaptProfitLossView(dto)
}

async function fetchCostEntryDetail(
    costEntryId: string,
): Promise<CostEntryDetail> {
    const dto = await apiGet<CostEntryDto>(
        `/admin/cost-entries/${encodeURIComponent(costEntryId)}`,
    )
    return mapCostEntry(dto)
}

export async function fetchCostEntriesForRow(
    costEntryIds: readonly string[],
): Promise<CostEntryDetail[]> {
    if (costEntryIds.length === 0) return []
    // 无批量端点时仍并发读取；任一正式成本事实缺失即整体失败，禁止展示残缺明细。
    return Promise.all(costEntryIds.map(fetchCostEntryDetail))
}

export async function startProfitLossExport(input: {
    query: ProfitLossQuery
    view: Pick<
        ProfitLossView,
        | "period"
        | "scope"
        | "formulaVersion"
        | "freshness"
        | "rows"
        | "fieldPermissions"
    >
    coverage: ProfitLossQuery["coverage"]
}): Promise<ProfitLossExportJob> {
    if (!input.view.fieldPermissions.canExport) {
        const err = {
            kind: "Validation" as const,
            message: "当前权限不允许导出",
            status: 403,
        }
        throw err
    }
    if (!input.query.periodBasis) {
        const err = {
            kind: "Validation" as const,
            message: "periodBasis 未明确，已阻断导出",
            status: 400,
        }
        throw err
    }

    const dto = await apiPost<ExportJobDto>(
        "/admin/actual-profit-loss/exports",
        {
            from: input.view.period.from,
            to: input.view.period.to,
            period_basis: input.view.period.basis,
            coverage: input.coverage,
            scope_id: input.view.scope.id,
            formula_version: input.view.formulaVersion,
            projection_watermark: input.view.freshness.sourceWatermark,
            projected_at: input.view.freshness.projectedAt,
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
            periodBasis: input.view.period.basis,
            formulaVersion: input.view.formulaVersion,
            coverage: input.coverage,
            scopeId: input.view.scope.id,
            scopeLabel: input.view.scope.label,
            permissionVersion: input.view.scope.permissionVersion,
            projectedAt: input.view.freshness.projectedAt,
            sourceWatermark: input.view.freshness.sourceWatermark,
            amountBasis: "NET",
            businessType: "GOODS_SERVICE",
            rowCount: input.view.rows.total,
        },
    }
}
