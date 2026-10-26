import { QueryClient, QueryObserver } from "@tanstack/react-query"
import { apiGet, apiPost } from "@/lib/api"
import { getToken } from "@/lib/api/session"
import { toast } from "@/components/ui/toast"
import {
    captureNavDeliveryOrigin,
    dispatchNavDelivery,
    type NavDeliveryPoint,
} from "@/lib/nav-delivery"

export type TransferKind =
    | "product_pool_excel"
    | "supplier_quote_archive"
    | "sales_orders"
    | "purchase_orders"
    | "contracts"
    | "customers"
    | "inventory_balance"
    | "inventory_movement"
    | "inventory_adjustment"
    | "inventory_reservation"
    | "supplier_orders"
    | "receivables"
    | "supplier_payables"
    | "invoice_requests"
    | "profit_loss"
    | "customer_quality_current"
    | "customer_quality_history"
    | "master_data"
    | "inquiry_preview"
    | "inquiry_creation_preview"
    | "supplier_quote_preview"
    | "supply_batch"
    | "portal_batch"
export type TransferJob = {
    id: string
    job_no: string
    status: string
    error_summary?: string | null
    processed_count: number
    total_count: number
    finished_at?: number | null
    result_file_asset_id?: string | null
}
export type TransferFile = {
    file_name: string
    content_type: string
    content_base64: string
    metadata?: Record<string, unknown>
}

/** 原任务的受控业务上下文；不包含上传文件或调用方指定的跳转地址。 */
export type DataTransferContext = {
    kind: TransferKind
    context: {
        inquiry_id?: string
        customer_id?: string
        task_id?: string
        expected_version?: number
        task_version?: number
        requirement_version?: number
        feedback_round?: number
        inquiry_no?: string
        file_name?: string
        file_names?: string[]
        mode?: string
        resource?: string
        view?: string
    }
    can_cancel: boolean
    can_recover: boolean
    resume_state:
        | "not_applicable"
        | "pending"
        | "ready"
        | "confirmed"
        | "invalid"
        | "stale"
        | "expired"
        | "unavailable"
    resume_reason: string | null
    preview_expires_at: number | null
}

export function readTransferContext(id: string, portal = false) {
    return apiGet<DataTransferContext>(
        `${base(portal)}/${encodeURIComponent(id)}/context`,
        undefined,
        options(portal),
    )
}

/** 仅按原任务查询结果待确认行的回执，不提交新的业务写入。 */
export function recoverDataTransfer(id: string, portal = false) {
    return apiPost<TransferFile>(
        `${base(portal)}/${encodeURIComponent(id)}/recover`,
        {},
        options(portal),
    )
}

export function decodeTransferJson<T>(file: TransferFile): T {
    return JSON.parse(transferText(file)) as T
}
const options = (portal: boolean) =>
    portal ? { session: "supplier-portal" as const } : undefined
const base = (portal: boolean) =>
    portal ? "/supplier-portal/data-transfers" : "/account/data-transfers"

/** 仅由业务 mutationFn 调用；状态轮询交由 Query 管理，断开等待不取消服务端任务。 */
export async function runDataTransfer(
    kind: TransferKind,
    input: unknown,
    requestId: string | undefined = undefined,
    portal = false,
    origin: NavDeliveryPoint = captureNavDeliveryOrigin(),
) {
    const identity = await transferIdentity(kind, input, portal, requestId)
    const job = await apiPost<TransferJob>(
        base(portal),
        { kind, input, request_id: identity.id },
        { ...options(portal), timeoutMs: 120_000 },
    )
    const delivered = dispatchNavDelivery({
        kind: portal ? "portal-task" : "file-task",
        origin,
        task: { id: job.id, jobNo: job.job_no },
    })
    if (!delivered) {
        toast.add({
            title: "已提交后台处理",
            description: `任务 ${job.job_no}，可离开当前页面，稍后到“我的文件任务”查看结果。`,
            type: "info",
        })
    }
    const completed = await waitForTransfer(job, portal)
    identity.clear()
    if (!completed.result_file_asset_id)
        throw new Error(
            completed.error_summary ||
                "后台任务未生成结果，请到任务入口查看详情",
        )
    const file = await readTransferResult(job.id, portal)
    return { ...file, jobId: job.id }
}
function waitForTransfer(
    initial: TransferJob,
    portal: boolean,
): Promise<TransferJob> {
    const client = new QueryClient()
    const observer = new QueryObserver(client, {
        queryKey: ["data-transfer", portal, initial.id],
        queryFn: () =>
            apiGet<TransferJob>(
                `${base(portal)}/${encodeURIComponent(initial.id)}`,
                undefined,
                options(portal),
            ),
        refetchInterval: 2000,
        refetchIntervalInBackground: true,
        retry: 2,
    })
    return new Promise((resolve, reject) => {
        const timer = setTimeout(
            () => finish(new Error("任务仍在后台处理，请到任务入口查看结果")),
            30 * 60 * 1000,
        )
        let unsubscribe = () => {}
        const finish = (error?: unknown, job?: TransferJob) => {
            clearTimeout(timer)
            unsubscribe()
            client.clear()
            if (error) reject(error)
            else if (job) resolve(job)
        }
        unsubscribe = observer.subscribe((state) => {
            if (state.isError) {
                finish(state.error)
                return
            }
            const job = state.data
            if (!job) return
            const status = job.status.toLowerCase()
            if (
                (job.finished_at && job.result_file_asset_id) ||
                ["succeeded", "completed"].includes(status)
            )
                finish(undefined, job)
            else if (["failed", "cancelled"].includes(status))
                finish(undefined, job)
        })
    })
}
export function readTransferResult(id: string, portal = false) {
    return apiGet<TransferFile>(
        `${base(portal)}/${encodeURIComponent(id)}/result`,
        undefined,
        { ...options(portal), timeoutMs: 170_000 },
    )
}
export function transferBytes(file: TransferFile) {
    return Uint8Array.from(atob(file.content_base64), (char) =>
        char.charCodeAt(0),
    )
}
export function transferText(file: TransferFile) {
    return new TextDecoder("utf-8", { ignoreBOM: true }).decode(
        transferBytes(file),
    )
}
export function downloadTransfer(file: TransferFile) {
    const url = URL.createObjectURL(
        new Blob([transferBytes(file)], { type: file.content_type }),
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = file.file_name
    anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export async function transferCsv(kind: TransferKind, input: unknown) {
    const file = await runDataTransfer(kind, input)
    const content = transferText(file)
    return {
        jobId: file.jobId,
        fileName: file.file_name,
        content,
        rowCount:
            typeof file.metadata?.row_count === "number"
                ? file.metadata.row_count
                : 0,
    }
}
export async function transferJson<T>(
    kind: TransferKind,
    input: unknown,
    requestId?: string,
    portal = false,
): Promise<T> {
    return JSON.parse(
        transferText(await runDataTransfer(kind, input, requestId, portal)),
    ) as T
}
export async function transferFileInput(file: File) {
    const bytes = new Uint8Array(await file.arrayBuffer())
    let binary = ""
    for (let i = 0; i < bytes.length; i += 8192)
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192))
    return { file_name: file.name, content_base64: btoa(binary) }
}

/** 仅保存请求身份摘要，创建响应丢失时沿用原操作号；不保存令牌、文件或业务数据。 */
async function transferIdentity(
    kind: TransferKind,
    input: unknown,
    portal: boolean,
    explicit?: string,
) {
    if (explicit) return { id: explicit, clear: () => {} }
    const digest = await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode(
            JSON.stringify([
                getToken(portal ? "supplier-portal" : "admin"),
                kind,
                input,
            ]),
        ),
    )
    const key = `transfer-request:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`
    let id = crypto.randomUUID() as string
    try {
        id = sessionStorage.getItem(key) ?? id
        sessionStorage.setItem(key, id)
    } catch {
        /* 浏览器禁用存储时仍可提交，并通过我的文件任务恢复。 */
    }
    return {
        id,
        clear: () => {
            try {
                sessionStorage.removeItem(key)
            } catch {
                /* 无持久状态可清理。 */
            }
        },
    }
}
