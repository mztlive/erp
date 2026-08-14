/**
 * P0-5 垂直样板：来源系统真实接口（D01 source_registry）类型与标签。
 */

/**
 * 来源系统类型。锁定契约：system_type ∈ ERP | MALL | SUPPLIER（serde snake_case）。
 * 展示必须走 SOURCE_SYSTEM_TYPE_LABEL 中文映射，禁止枚举原值上屏（AGENTS.md §5）。
 */
export type SourceSystemType = "ERP" | "MALL" | "SUPPLIER"

/**
 * 来源系统状态。锁定契约：status ∈ 启用 | 停用（后端直接下发中文状态文案）。
 * 若后端实际产物与此不符（如改为 ACTIVE/INACTIVE 枚举码），需同步调整
 * SOURCE_SYSTEM_STATUS_LABEL 与 api.ts 的契约注释。
 */
export type SourceSystemStatus = "启用" | "停用"

/** 来源系统列表行（GET /admin/source-systems 的 items 项）。 */
export type SourceSystemItem = {
    id: string
    code: string
    name: string
    system_type: SourceSystemType
    status: SourceSystemStatus
    /** 创建时间：api-contract.md §5.1 约定秒级 Unix 时间戳；后端若返回 ISO 字符串则原样透传 */
    created_at: number | string
}

/** 来源系统分页查询参数（page 从 1 起，对齐 lib/api/paging.ts 的 PageParams）。 */
export type SourceSystemListParams = {
    page: number
    page_size: number
}

/** 来源系统分页响应（对齐 api-contract.md §3 分页形状）。 */
export type SourceSystemPage = {
    items: SourceSystemItem[]
    total: number
    page: number
    page_size: number
}

/** 来源系统类型中文映射（AGENTS.md §5：新增枚举必须同时写中文映射表）。 */
export const SOURCE_SYSTEM_TYPE_LABEL: Record<SourceSystemType, string> = {
    ERP: "ERP",
    MALL: "商城",
    SUPPLIER: "供应商",
}

/** 来源系统状态中文映射（契约值为中文；映射表为未知值兜底保留）。 */
export const SOURCE_SYSTEM_STATUS_LABEL: Record<SourceSystemStatus, string> = {
    启用: "启用",
    停用: "停用",
}
