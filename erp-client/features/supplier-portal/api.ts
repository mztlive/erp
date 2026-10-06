import {
    apiGet,
    apiGetBlob,
    apiPost,
    apiPostForm,
    apiPut,
    setToken,
} from "@/lib/api"
import type {
    NewProductInput,
    PortalApplication,
    PortalBatchMode,
    PortalBatchPhase,
    PortalBatchResult,
    PortalBatchRow,
    PortalCatalogSku,
    PortalCategoryMappingSuggestion,
    PortalCooperation,
    PortalDictionary,
    PortalOffering,
    PortalPage,
    PortalProfile,
    PortalUpload,
} from "./types"

const options = { session: "supplier-portal" } as const
const target = (resource: string, id: string) =>
    `/supplier-portal/${resource}/${encodeURIComponent(id)}`
export async function portalLogin(input: {
    account: string
    password: string
}) {
    const result = await apiPost<{ token: string; profile?: PortalProfile }>(
        "/supplier-portal/login",
        input,
        { session: "none" },
    )
    if (!result.token) throw new Error("登录结果缺少凭证，请重试")
    setToken(result.token, "supplier-portal")
    return result
}
export const portalSession = () =>
    apiGet<PortalProfile>("/supplier-portal/session", undefined, options)
export const portalOfferings = (query: Record<string, unknown>) =>
    apiGet<PortalPage<PortalOffering>>(
        "/supplier-portal/offerings",
        query,
        options,
    )
export const portalOffering = (id: string) =>
    apiGet<PortalOffering>(target("offerings", id), undefined, options)
export const portalOfferingHistory = (id: string) =>
    apiGet<
        {
            id: string
            revision_no: number
            terms: PortalOffering["terms"]
            created_at?: string
        }[]
    >(`${target("offerings", id)}/revisions`, undefined, options)
export const portalAvailability = (
    id: string,
    input: Record<string, unknown>,
) => apiPost<unknown>(`${target("offerings", id)}/availability`, input, options)
export const portalCatalog = (query: Record<string, unknown>) =>
    apiGet<PortalPage<PortalCatalogSku>>(
        "/supplier-portal/catalog",
        query,
        options,
    )
/** 以开放目录实际返回的精确身份读取目标，不假定目标在第一页。 */
export async function portalCatalogTarget(
    id: string,
): Promise<PortalCatalogSku | null> {
    for (let page = 1; ; page++) {
        const result = await portalCatalog({ page, page_size: 100 })
        const target = result.items.find((item) => item.id === id)
        if (target) return target
        if (!result.items.length || page * 100 >= result.total) return null
    }
}
export const portalDictionaries = (kind: string) =>
    apiGet<PortalDictionary[]>(
        `/supplier-portal/dictionaries/${kind}`,
        undefined,
        options,
    )
export function mapPortalApplication(
    dto: Record<string, unknown>,
): PortalApplication {
    const submissions = Array.isArray(dto.submissions)
        ? (dto.submissions as Record<string, unknown>[])
        : []
    const latest = submissions.at(-1)
    const decisions = Array.isArray(dto.decisions)
        ? (dto.decisions as Record<string, unknown>[])
        : submissions.flatMap((row) =>
              row.decision && typeof row.decision === "object"
                  ? [row.decision as Record<string, unknown>]
                  : [],
          )
    const kindMap: Record<string, PortalApplication["kind"]> = {
        EXISTING_QUOTE: "quote",
        TERMS_CHANGE: "terms",
        STOP_SUPPLY: "stop",
        NEW_PRODUCT: "new_product",
        COOPERATION: "cooperation",
        PAYMENT_TERMS: "cooperation",
    }
    const statusMap: Record<string, PortalApplication["status"]> = {
        DRAFT: "draft",
        SUBMITTED: "pending",
        RETURNED: "returned",
        WITHDRAWN: "withdrawn",
        EFFECTIVE: "effective",
    }
    return {
        ...dto,
        reason:
            dto.reason ??
            (dto.input as Record<string, unknown> | undefined)?.reason ??
            (dto.proposal as Record<string, unknown> | undefined)?.reason ??
            "",
        kind: kindMap[String(dto.kind)] ?? dto.kind,
        status: statusMap[String(dto.status)] ?? dto.status,
        input: dto.input ?? dto.snapshot ?? dto.draft ?? dto.proposal ?? {},
        submitted_snapshot:
            dto.frozen_input ??
            latest?.snapshot ??
            latest?.input ??
            latest?.proposal,
        work_item_id:
            dto.work_item_id ??
            dto.task_id ??
            latest?.work_item_id ??
            latest?.task_id,
        work_item_version: dto.work_item_version ?? latest?.work_item_version,
        decisions: decisions.map((d) => ({
            decision:
                statusMap[String(d.status).toUpperCase()] ??
                String(d.decision ?? d.status ?? ""),
            comment: String(d.reason ?? d.comment ?? ""),
            at: d.decided_at as number | string | undefined,
        })),
    } as PortalApplication
}
export const portalApplications = async (query: Record<string, unknown>) => {
    const page = await apiGet<PortalPage<Record<string, unknown>>>(
        "/supplier-portal/applications",
        query,
        options,
    )
    return { ...page, items: page.items.map(mapPortalApplication) }
}
export const portalApplication = async (id: string) =>
    mapPortalApplication(
        await apiGet<Record<string, unknown>>(
            target("applications", id),
            undefined,
            options,
        ),
    )
export const portalSaveApplication = async (
    input: Record<string, unknown>,
    id?: string,
) =>
    mapPortalApplication(
        await (id
            ? apiPut<Record<string, unknown>>(
                  target("applications", id),
                  input,
                  options,
              )
            : apiPost<Record<string, unknown>>(
                  "/supplier-portal/applications",
                  input,
                  options,
              )),
    )
export const portalSaveNewProduct = async (
    input: NewProductInput,
    idempotencyKey: string,
) =>
    mapPortalApplication(
        await apiPost<Record<string, unknown>>(
            "/supplier-portal/new-products",
            { input, idempotency_key: idempotencyKey },
            options,
        ),
    )
export const portalApplicationAction = async (
    id: string,
    action: "submit" | "withdraw",
    input: Record<string, unknown>,
) =>
    mapPortalApplication(
        await apiPost<Record<string, unknown>>(
            `${target("applications", id)}/${action}`,
            input,
            options,
        ),
    )
export const portalCooperation = async () => {
    const value = await apiGet<
        PortalCooperation & {
            current_profile_id?: string
            procurement_contact_name?: string
        }
    >("/supplier-portal/cooperation", undefined, options)
    return {
        ...value,
        profile_id: value.current_profile_id ?? value.profile_id,
        contact_name: value.procurement_contact_name ?? value.contact_name,
    }
}
export const portalCooperationApplication = async (
    input: Record<string, unknown>,
) =>
    mapPortalApplication(
        await apiPost<Record<string, unknown>>(
            "/supplier-portal/cooperation/applications",
            input,
            options,
        ),
    )
export const portalChangePassword = (input: Record<string, unknown>) =>
    apiPost<unknown>("/supplier-portal/password", input, options)
export const portalBatch = (
    mode: PortalBatchMode,
    rows: PortalBatchRow[],
    validateOnly: boolean,
    recoveryOnly = false,
    phase?: PortalBatchPhase,
) =>
    apiPost<PortalBatchResult>(
        "/supplier-portal/batch",
        {
            mode,
            rows,
            validate_only: validateOnly,
            recovery_only: recoveryOnly,
            ...(mode === "new_product" ? { phase: phase ?? "prepare" } : {}),
        },
        options,
    )
export const portalUpload = (
    applicationId: string,
    file: File,
    expectedVersion: number,
    idempotencyKey: string,
) => {
    const form = new FormData()
    form.append("file", file)
    return apiPostForm<PortalUpload>(
        `${target("applications", applicationId)}/files?${new URLSearchParams({ expected_version: String(expectedVersion), idempotency_key: idempotencyKey })}`,
        form,
        options,
    )
}
export type PortalFileSource =
    | { request_id: string }
    | { offering_id: string }
    | { sku_id: string }
export const portalFile = (source: PortalFileSource, fileId: string) =>
    apiGetBlob(
        `/supplier-portal/files/${encodeURIComponent(fileId)}/download?${new URLSearchParams(source)}`,
        options,
    )

export const portalCategoryMappingSuggestion = (query: {
    original_category_path: string
    product_kind: string
}) =>
    apiGet<PortalCategoryMappingSuggestion | null>(
        "/supplier-portal/category-mapping-suggestion",
        query,
        options,
    )
