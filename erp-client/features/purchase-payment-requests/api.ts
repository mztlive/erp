import {
    apiGet,
    apiGetBlob,
    apiPost,
    apiPostForm,
    apiPut,
    type Page,
} from "@/lib/api"
import { paymentTermLabel } from "@/lib/business-options"
import { BANK_RECEIPT_PENDING_REFERENCE } from "@/features/supplier-payables/lib/allocation-model"
import { subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import type {
    PaymentCandidate,
    PaymentInput,
    PaymentRecipient,
    PaymentRequest,
    RequestAttachment,
    RequestCommand,
    RequestDraftInput,
    RequestMaterials,
    RequestPaymentRecord,
    RequestStatus,
    SalesSnapshot,
    PurchaseSnapshot,
} from "./types"

const ROOT = "/admin/purchase-payment-requests"
const moneyOptions = { maxScale: 2, outputScale: 2 } as const
type PaymentTerms = {
    payment_term_name?: string
    payment_term_code?: string
    prepay_gate?: boolean
    prepay_minimum_amount?: string | null
    prepay_minimum_ratio?: string | null
}
function termLabel(terms: PaymentTerms) {
    return (
        terms.payment_term_name ||
        (terms.payment_term_code
            ? paymentTermLabel(terms.payment_term_code)
            : "未标注付款条件")
    )
}
type BankAccount = {
    id: string
    version: number
    account_name: string
    bank_name: string
    bank_branch_name?: string | null
    account_number_masked: string
    is_default?: boolean
}
type CandidateDto = {
    payable_account_id: string
    purchase_order_id: string
    purchase_no: string
    purchase_revision_id: string
    supplier_id: string
    supplier_name: string
    payer_party_id: string
    payer_party_name: string
    gross_total: string
    settled_total: string
    open_total: string
    reserved_total: string
    available_total: string
    payment_terms: PaymentTerms
    bank_accounts: BankAccount[]
}
type PurchasePreviewDto = {
    purchase_order_id: string
    purchase_no: string
    revision_id: string
    revision_no: number
    supplier_name: string
    payment_term_snapshot: PaymentTerms
    totals: { gross: string; net: string; tax: string }
    lines: {
        line_id: string
        product_name?: string | null
        specification?: string | null
        quantity?: string | null
        base_unit_code?: string | null
        unit_cost_gross?: string | null
        gross_amount: string
    }[]
}
type RequestLineDto = {
    payable_account_id: string
    purchase_order_id: string
    purchase_revision_id: string
    requested_amount: string
    paid_amount: string
    closed_amount: string
    gross_total: string
    settled_total: string
    open_total: string
    other_reserved_total: string
    payment_terms: PaymentTerms
    source_snapshot: {
        purchase_preview: PurchasePreviewDto
        allowed_attachment_ids: string[]
    }
}
type RequestDto = {
    id: string
    version: number
    submission_version: number
    request_no: string
    status: RequestStatus
    supplier_id: string
    supplier_name?: string
    payer_party_id: string
    payer_party_name: string
    created_by_name?: string
    payee_account?: {
        account_id: string
        version: number
        account_name: string
        bank_name: string
        masked_account_number: string
    } | null
    data: {
        supplier_id: string
        payee_bank_account_id: string
        desired_payment_date: string
        reason: string
        attachment_ids: string[]
    }
    total_amount: string
    last_reason?: string | null
    lines: RequestLineDto[]
    approval?: DocumentApprovalViewDto | null
    allowed_actions?: string[]
    purchase_previews?: PurchasePreviewDto[]
    sales_previews?: SalesSnapshot[]
    attachments?: RequestAttachment[]
    payments?: RequestPaymentRecord[]
    current_lines?: {
        purchase_order_id: string
        current_open_total: string
        current_reserved_total: string
        receiving_progress: string
        source_changed: boolean
        payable_entries: { id: string; open_total: string }[]
    }[]
}

function recipient(account: BankAccount): PaymentRecipient {
    return { ...account, bank_account_id: account.id }
}
function mapCandidate(item: CandidateDto): PaymentCandidate {
    const accounts = item.bank_accounts.map(recipient)
    return {
        payable_account_id: item.payable_account_id,
        purchase_order_id: item.purchase_order_id,
        purchase_order_no: item.purchase_no,
        purchase_version: 0,
        supplier_id: item.supplier_id,
        supplier_name: item.supplier_name,
        our_company_id: item.payer_party_id,
        our_company_name: item.payer_party_name,
        payment_term_name: termLabel(item.payment_terms),
        total_amount: item.gross_total,
        paid_amount: item.settled_total,
        open_amount: item.open_total,
        reserved_amount: item.reserved_total,
        available_amount: item.available_total,
        payment_recipient:
            accounts.find(
                (_, index) => item.bank_accounts[index]?.is_default,
            ) ?? accounts[0],
        bank_accounts: accounts,
    }
}
function mapRequest(dto: RequestDto): PaymentRequest {
    const paid = sumFixed(
        dto.lines.map((line) => line.paid_amount),
        moneyOptions,
    )
    const closed = sumFixed(
        dto.lines.map((line) => line.closed_amount),
        moneyOptions,
    )
    const unpaid = subtractFixed(
        subtractFixed(dto.total_amount, paid, moneyOptions),
        closed,
        moneyOptions,
    )
    return {
        id: dto.id,
        version: dto.version,
        submission_version: dto.submission_version,
        request_no: dto.request_no,
        status: dto.status,
        supplier_id: dto.supplier_id,
        supplier_name:
            dto.supplier_name ||
            dto.purchase_previews?.[0]?.supplier_name ||
            "未记录供应商名称",
        our_company_id: dto.payer_party_id,
        our_company_name: dto.payer_party_name,
        applicant_name: dto.created_by_name || "未记录姓名",
        desired_payment_date: dto.data.desired_payment_date,
        description: dto.data.reason,
        payment_recipient: {
            bank_account_id:
                dto.payee_account?.account_id ?? dto.data.payee_bank_account_id,
            version: dto.payee_account?.version ?? 0,
            account_name: dto.payee_account?.account_name ?? "未记录",
            bank_name: dto.payee_account?.bank_name ?? "未记录",
            account_number_masked:
                dto.payee_account?.masked_account_number ?? "未记录",
        },
        total_amount: dto.total_amount,
        paid_amount: paid,
        closed_amount: closed,
        unpaid_amount: unpaid,
        close_reason: dto.last_reason ?? undefined,
        attachment_ids: dto.data.attachment_ids,
        attachments: dto.attachments ?? [],
        approval: dto.approval,
        allowed_actions: dto.allowed_actions ?? [],
        payments: dto.payments ?? [],
        lines: dto.lines.map((line) => {
            const preview =
                (dto.purchase_previews ?? []).find(
                    (item) => item.purchase_order_id === line.purchase_order_id,
                ) ?? line.source_snapshot.purchase_preview
            const current = dto.current_lines?.find(
                (item) => item.purchase_order_id === line.purchase_order_id,
            )
            return {
                payable_account_id: line.payable_account_id,
                purchase_order_id: line.purchase_order_id,
                purchase_order_no: preview.purchase_no,
                purchase_version: preview.revision_no,
                supplier_id: dto.supplier_id,
                supplier_name: dto.supplier_name || preview.supplier_name,
                our_company_id: dto.payer_party_id,
                our_company_name: dto.payer_party_name,
                payment_term_name: termLabel(line.payment_terms),
                total_amount: line.gross_total,
                open_amount: line.open_total,
                reserved_amount: line.other_reserved_total,
                available_amount: line.open_total,
                bank_accounts: [],
                requested_amount: line.requested_amount,
                paid_amount: line.paid_amount,
                source_paid_amount: line.settled_total,
                source_changed: current?.source_changed,
                payable_entries: current?.payable_entries ?? [],
                closed_amount: line.closed_amount,
                unpaid_amount: subtractFixed(
                    subtractFixed(
                        line.requested_amount,
                        line.paid_amount,
                        moneyOptions,
                    ),
                    line.closed_amount,
                    moneyOptions,
                ),
                current_open_amount: current?.current_open_total,
                current_reserved_amount: current?.current_reserved_total,
                current_fulfillment: current?.receiving_progress,
            }
        }),
    }
}
export async function fetchCandidates() {
    return (await apiGet<CandidateDto[]>(`${ROOT}/candidates`)).map(
        mapCandidate,
    )
}
export async function fetchRequests() {
    const rows: RequestDto[] = []
    for (let page = 1; page <= 100; page += 1) {
        const dto = await apiGet<Page<RequestDto>>(ROOT, {
            page,
            page_size: 100,
        })
        rows.push(...dto.items)
        if (rows.length >= dto.total || dto.items.length === 0) break
    }
    return rows.map(mapRequest)
}
function readRequestDto(id: string, workItemId?: string) {
    return apiGet<RequestDto>(
        workItemId
            ? `/admin/work-items/${encodeURIComponent(workItemId)}/purchase-payment-request`
            : `${ROOT}/${encodeURIComponent(id)}`,
    )
}
export async function fetchRequest(id: string, workItemId?: string) {
    return mapRequest(await readRequestDto(id, workItemId))
}
function readSubmissionDto(id: string, submissionVersion: number) {
    return apiGet<RequestDto>(
        `${ROOT}/${encodeURIComponent(id)}/submissions/${submissionVersion}`,
    )
}
export async function fetchSubmission(id: string, submissionVersion: number) {
    return mapRequest(await readSubmissionDto(id, submissionVersion))
}
export async function saveRequest(
    input: RequestDraftInput,
    id?: string,
    version?: number,
) {
    const body = {
        expected_version: version,
        data: {
            supplier_id: input.supplier_id,
            payee_bank_account_id: input.bank_account_id,
            lines: input.lines,
            desired_payment_date: input.desired_payment_date,
            reason: input.description,
            attachment_ids: input.attachment_ids,
        },
    }
    const dto = id
        ? await apiPut<RequestDto>(`${ROOT}/${encodeURIComponent(id)}`, body)
        : await apiPost<RequestDto>(ROOT, body)
    return mapRequest(dto)
}
export async function commandRequest(
    id: string,
    action: "submit" | "withdraw" | "close",
    command: RequestCommand,
) {
    await apiPost(`${ROOT}/${encodeURIComponent(id)}/${action}`, command)
}
export async function fetchMaterials(
    id: string,
    workItemId?: string,
    submissionVersion?: number,
): Promise<RequestMaterials> {
    const dto =
        submissionVersion != null
            ? await readSubmissionDto(id, submissionVersion)
            : await readRequestDto(id, workItemId)
    return {
        sales: dto.sales_previews ?? [],
        purchases: (dto.purchase_previews ?? []).map((preview) => ({
            purchase_order_id: preview.purchase_order_id,
            purchase_order_no: preview.purchase_no,
            version: preview.revision_no,
            snapshot: {
                supplier_name: preview.supplier_name,
                our_company_name: dto.payer_party_name,
                payment_term_name: termLabel(preview.payment_term_snapshot),
                total_amount: preview.totals.gross,
                items: preview.lines.map((line) => ({
                    id: line.line_id,
                    item_name: line.product_name || "物流费用",
                    spec: line.specification ?? undefined,
                    quantity: line.quantity ?? "1",
                    unit: line.base_unit_code ?? undefined,
                    unit_price: line.unit_cost_gross ?? line.gross_amount,
                    amount: line.gross_amount,
                })),
            } satisfies PurchaseSnapshot,
        })),
    }
}
export async function commitPayment(
    input: PaymentInput,
    account: PaymentRecipient,
) {
    const form = new FormData()
    form.append(
        "command",
        JSON.stringify({
            payment_request_id: input.requestId,
            expected_request_version: input.expectedVersion,
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_payee_bank_account_id: account.bank_account_id,
            expected_payee_bank_account_version: account.version,
            payment: {
                payment_no: `FK-${input.idempotencyKey.slice(-8)}`,
                supplier_id: input.supplierId,
                paid_at: Math.floor(new Date(input.paidAt).getTime() / 1000),
                amount: input.amount,
                bank_reference: input.bankReference || undefined,
                bank_receipt_asset_id: BANK_RECEIPT_PENDING_REFERENCE,
            },
            allocations: input.allocations,
            idempotency_key: input.idempotencyKey,
        }),
    )
    form.append(
        BANK_RECEIPT_PENDING_REFERENCE,
        input.receipt,
        input.receipt.name,
    )
    return apiPostForm<{ id: string; payment_no: string }>(
        `${ROOT}/${encodeURIComponent(input.requestId)}/payments/commit`,
        form,
    )
}
export async function downloadAttachment(
    requestId: string,
    attachment: RequestAttachment,
    workItemId?: string,
    submissionVersion?: number,
) {
    const query =
        submissionVersion != null
            ? `?submission_version=${submissionVersion}`
            : workItemId
              ? `?work_item_id=${encodeURIComponent(workItemId)}`
              : ""
    const blob = await apiGetBlob(
        `${ROOT}/${encodeURIComponent(requestId)}/materials/${encodeURIComponent(attachment.file_asset_id)}/download${query}`,
        { cache: "no-store" },
    )
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = attachment.file_name
    anchor.click()
    URL.revokeObjectURL(url)
}
export async function revealAccount(
    request: PaymentRequest,
    task: { workItemId: string; taskVersion: string },
) {
    const result = await apiPost<{ account_number: string }>(
        `${ROOT}/${encodeURIComponent(request.id)}/reveal-account`,
        {
            work_item_id: task.workItemId,
            expected_task_version: task.taskVersion,
            expected_bank_account_id: request.payment_recipient.bank_account_id,
            expected_bank_account_version: request.payment_recipient.version,
        },
    )
    return result.account_number
}
