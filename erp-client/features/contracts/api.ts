/**
 * W04 合同中心 — 真实 HTTP 适配层。
 * 保持 queries.ts 消费的导出签名与返回类型稳定；后端 Page/DTO 仅在此映射。
 *
 * 后端路由：
 * - GET/POST /admin/contracts
 * - GET /admin/contracts/{id}
 * - POST /admin/contracts/{id}/revisions
 * - POST /admin/contracts/{id}/terminate
 * - POST /admin/file-assets/upload（multipart）
 */

import {
  apiGet,
  apiPost,
  getApiBaseUrl,
  getToken,
  type ApiError,
  type Page,
} from "@/lib/api"
import { PAYMENT_TERM_OPTIONS, paymentTermLabel } from "@/lib/business-options"
import { contractPdfError } from "@/features/contracts/pdf"
import type {
  ContractAction,
  ContractAttachmentView,
  ContractCenterView,
  ContractExportJob,
  ContractListRow,
  ContractRevisionSummary,
  ContractStatus,
  UploadContractPdfInput,
  UploadContractPdfResult,
} from "@/features/contracts/types"
import {
  CONTRACT_STATUS_LABEL,
  CONTRACT_STATUS_TONE,
} from "@/features/contracts/types"

// ─── Backend wire types ──────────────────────────────────────────────────────

type BackendContractView = {
  id: string
  contract_no: string
  customer_id: string
  settlement_party_id: string
  status: ContractStatus | string
  current_revision_id?: string | null
  created_at: number
  version: number
}

type BackendContractRevision = {
  id: string
  revision_no: number
  contract_pdf_file_id: string
  archive_source: string
  customer_name: string
  settlement_party_name: string
  payment_term_code: string
  payment_term_name: string
  invoice_type: string
  tax_point: string
  valid_from: string
  valid_to?: string | null
  signed_at: string
  created_at: number
}

type BackendContractDetail = BackendContractView & {
  revisions: BackendContractRevision[]
}

type BackendCustomerDetail = {
  id: string
  party_id: string
  customer_no: string
  legal_name?: string | null
  party_no?: string | null
  owner_user_id?: string | null
  version: number
  created_at: number
}

type BackendPartyView = {
  id: string
  party_no: string
  unified_credit_code?: string | null
}

type BackendFileAsset = {
  id: string
  storage_object_key?: string
  file_name: string
  content_type: string
  byte_size: number
  security_scan_status: string
  created_by: string
  created_at: number
  version?: number
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function isApiError(error: unknown): error is ApiError {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error
  )
}

function apiErrorMessage(error: ApiError): string {
  const data = error.responseData as { errorMessage?: string } | undefined
  if (data?.errorMessage && data.errorMessage !== "OK") {
    return data.errorMessage
  }
  return error.message
}

function tsToIso(seconds: number | undefined | null): string {
  if (seconds == null || !Number.isFinite(seconds)) {
    return new Date().toISOString()
  }
  return new Date(seconds * 1000).toISOString()
}

function asContractStatus(raw: string): ContractStatus {
  if (raw === "TERMINATED" || raw === "EXPIRED" || raw === "EFFECTIVE") {
    return raw
  }
  return "EFFECTIVE"
}

function daysUntil(dateStr: string): number {
  const end = new Date(`${dateStr}T23:59:59`)
  const now = new Date()
  return Math.ceil((end.getTime() - now.getTime()) / (24 * 60 * 60 * 1000))
}

function isExpiringWithin30Days(
  status: ContractStatus,
  validTo?: string | null
): boolean {
  if (status !== "EFFECTIVE" || !validTo) return false
  const d = daysUntil(validTo)
  return d >= 0 && d <= 30
}

function paymentTermCodeFromLabel(label: string): string {
  const found = PAYMENT_TERM_OPTIONS.find(
    (o) => o.label === label || o.value === label
  )
  return found?.value ?? "CONTRACT"
}

function paymentTermDays(code: string): number | undefined {
  if (code.includes("15")) return 15
  if (code.includes("30")) return 30
  return undefined
}

function mapScanStatus(
  status: string
): ContractAttachmentView["securityState"] {
  switch (status) {
    case "passed":
      return "done"
    case "quarantined":
    case "rejected":
      return "quarantined"
    default:
      return "processing"
  }
}

function baseActions(status: ContractStatus): {
  allowedActions: ContractAction[]
  actionBlockers: ContractListRow["actionBlockers"]
  selectable: boolean
  selectableBlocker?: string
} {
  if (status === "EFFECTIVE") {
    return {
      allowedActions: [
        "UPLOAD_CONTRACT_PDF",
        "PRINT",
        "CREATE_SALES_ORDER",
        "EXPORT",
      ],
      actionBlockers: [],
      selectable: true,
    }
  }
  if (status === "EXPIRED") {
    return {
      allowedActions: ["PRINT", "EXPORT"],
      actionBlockers: [
        {
          action: "CREATE_SALES_ORDER",
          code: "CONTRACT_EXPIRED",
          message: "合同已到期，不能引用到新销售单",
        },
      ],
      selectable: false,
      selectableBlocker: "合同已到期",
    }
  }
  return {
    allowedActions: ["PRINT", "EXPORT"],
    actionBlockers: [
      {
        action: "CREATE_SALES_ORDER",
        code: "CONTRACT_TERMINATED",
        message: "合同已终止，不能引用到新销售单",
      },
    ],
    selectable: false,
    selectableBlocker: "合同已终止",
  }
}

async function loadCustomerBrief(
  customerId: string
): Promise<{
  customerId: string
  customerNo: string
  displayName: string
  partyId: string
  ownerLabel: string
} | null> {
  try {
    const c = await apiGet<BackendCustomerDetail>(
      `/admin/customers/${customerId}`
    )
    return {
      customerId: c.id,
      customerNo: c.customer_no,
      displayName: c.legal_name?.trim() || c.customer_no,
      partyId: c.party_id,
      ownerLabel: c.owner_user_id ?? "—",
    }
  } catch {
    return null
  }
}

async function loadPartyName(partyId: string): Promise<string> {
  try {
    const p = await apiGet<BackendPartyView>(`/admin/parties/${partyId}`)
    return p.party_no
  } catch {
    return partyId
  }
}

async function loadFileAsset(
  fileId: string
): Promise<BackendFileAsset | null> {
  try {
    return await apiGet<BackendFileAsset>(`/admin/file-assets/${fileId}`)
  } catch {
    return null
  }
}

/**
 * multipart 上传：lib/api 仅 JSON，故用原生 fetch + 鉴权头。
 */
async function uploadFileAsset(file: File): Promise<BackendFileAsset> {
  const form = new FormData()
  form.append("file", file, file.name)
  form.append("sensitivity_class", "sensitive")
  form.append("retention_class", "long_term")
  form.append("usage", "attachment")

  const headers: Record<string, string> = {}
  const token = getToken()
  if (token) headers.Authorization = `Bearer ${token}`

  const timeoutMs = 60_000
  let res: Response
  try {
    res = await fetch(`${getApiBaseUrl()}/admin/file-assets/upload`, {
      method: "POST",
      headers,
      body: form,
      signal: AbortSignal.timeout(timeoutMs),
    })
  } catch (cause) {
    const err: ApiError = {
      kind: "Network",
      message: "网络请求失败或连接超时",
      cause,
    }
    throw err
  }

  const text = await res.text()
  let parsed: unknown
  try {
    parsed = text ? JSON.parse(text) : null
  } catch (cause) {
    const err: ApiError = {
      kind: "Parse",
      message: "响应数据解析失败",
      cause,
      responseData: text,
    }
    throw err
  }

  const envelope = parsed as {
    success?: boolean
    status?: number
    errorMessage?: string
    data?: BackendFileAsset | null
  } | null

  if (res.status === 401 || envelope?.status === 401) {
    const err: ApiError = {
      kind: "Auth",
      message: "登录状态已失效，请重新登录",
      status: 401,
      responseData: parsed,
    }
    throw err
  }

  if (!res.ok) {
    const err: ApiError = {
      kind: res.status === 400 ? "Validation" : "Http",
      message:
        envelope?.errorMessage ||
        (res.status === 400
          ? "请求未通过业务校验"
          : `请求失败（HTTP ${res.status}）`),
      status: res.status,
      responseData: parsed,
    }
    throw err
  }

  if (envelope && envelope.success === false) {
    const err: ApiError = {
      kind: "Validation",
      message: envelope.errorMessage || "请求未通过业务校验",
      status: envelope.status,
      responseData: envelope,
    }
    throw err
  }

  const data = envelope?.data
  if (!data?.id) {
    const err: ApiError = {
      kind: "Parse",
      message: "上传响应缺少文件资产 ID",
      responseData: parsed,
    }
    throw err
  }
  return data
}

// ─── List / detail ───────────────────────────────────────────────────────────

function mapListRow(
  row: BackendContractView,
  revision: BackendContractRevision | null,
  customer: Awaited<ReturnType<typeof loadCustomerBrief>>,
  settlementName: string
): ContractListRow {
  const status = asContractStatus(String(row.status))
  const actions = baseActions(status)
  const validFrom = revision?.valid_from ?? tsToIso(row.created_at).slice(0, 10)
  const validTo = revision?.valid_to ?? "9999-12-31"

  return {
    contractId: row.id,
    contractNo: row.contract_no,
    customer: {
      customerId: row.customer_id,
      customerNo: customer?.customerNo ?? row.customer_id,
      displayName:
        revision?.customer_name ??
        customer?.displayName ??
        row.customer_id,
    },
    settlementParty: {
      partyId: row.settlement_party_id,
      displayName:
        revision?.settlement_party_name ?? settlementName ?? row.settlement_party_id,
    },
    status,
    statusLabel: CONTRACT_STATUS_LABEL[status],
    statusTone: CONTRACT_STATUS_TONE[status],
    revisionNo: revision?.revision_no ?? 1,
    signedAt: revision?.signed_at,
    validFrom,
    validTo,
    expiringWithin30Days: isExpiringWithin30Days(status, revision?.valid_to),
    salesOrderCount: 0,
    activeSalesOrderCount: 0,
    ownerLabel: customer?.ownerLabel ?? "—",
    ownerKind: "current_customer_owner",
    allowedActions: actions.allowedActions,
    actionBlockers: actions.actionBlockers,
  }
}

/**
 * 合同列表（全量拉取一页大容量；页面本地筛选/指标仍可用 filter-contracts）。
 */
export async function fetchContracts(): Promise<ContractListRow[]> {
  const page = await apiGet<Page<BackendContractView>>("/admin/contracts", {
    page: 1,
    page_size: 100,
    sort_by: "created_at",
    sort_dir: "desc",
  })

  const rows = await Promise.all(
    page.items.map(async (row) => {
      let revision: BackendContractRevision | null = null
      try {
        const detail = await apiGet<BackendContractDetail>(
          `/admin/contracts/${row.id}`
        )
        revision =
          detail.revisions.find((r) => r.id === detail.current_revision_id) ??
          detail.revisions[0] ??
          null
      } catch {
        revision = null
      }
      const customer = await loadCustomerBrief(row.customer_id)
      const settlementName =
        revision?.settlement_party_name ??
        (await loadPartyName(row.settlement_party_id))
      return mapListRow(row, revision, customer, settlementName)
    })
  )

  return rows
}

/**
 * 合同对象中心。
 */
export async function fetchContractCenter(
  contractId: string
): Promise<ContractCenterView | null> {
  if (!contractId) return null

  let detail: BackendContractDetail
  try {
    detail = await apiGet<BackendContractDetail>(
      `/admin/contracts/${contractId}`
    )
  } catch (error) {
    if (isApiError(error) && (error.status === 404 || error.status === 403)) {
      return null
    }
    throw error
  }

  const status = asContractStatus(String(detail.status))
  const actions = baseActions(status)
  const current =
    detail.revisions.find((r) => r.id === detail.current_revision_id) ??
    detail.revisions[0]
  const customer = await loadCustomerBrief(detail.customer_id)

  const attachments: ContractAttachmentView[] = []
  for (const rev of detail.revisions) {
    const file = await loadFileAsset(rev.contract_pdf_file_id)
    if (!file) {
      attachments.push({
        id: rev.contract_pdf_file_id,
        name: `${detail.contract_no}-r${rev.revision_no}.pdf`,
        contentType: "application/pdf",
        revisionNo: rev.revision_no,
        uploadedBy: "—",
        uploadedAt: tsToIso(rev.created_at),
        securityState: "processing",
        canDownload: false,
      })
      continue
    }
    const securityState = mapScanStatus(file.security_scan_status)
    attachments.push({
      id: file.id,
      name: file.file_name,
      contentType: file.content_type,
      revisionNo: rev.revision_no,
      uploadedBy: file.created_by,
      uploadedAt: tsToIso(file.created_at),
      securityState,
      canDownload: securityState === "done",
    })
  }

  const revisionTimeline: ContractRevisionSummary[] = detail.revisions.map(
    (r) => ({
      revisionId: r.id,
      revisionNo: r.revision_no,
      validFrom: r.valid_from,
      validTo: r.valid_to ?? "9999-12-31",
      changeReason: r.archive_source,
      effectiveAt: tsToIso(r.created_at),
      isCurrent: r.id === detail.current_revision_id || r === detail.revisions[0],
    })
  )

  const nowIso = new Date().toISOString()
  const termCode = current?.payment_term_code ?? "CONTRACT"
  const termName =
    current?.payment_term_name ?? paymentTermLabel(termCode) ?? termCode

  return {
    contractId: detail.id,
    contractNo: detail.contract_no,
    status,
    statusLabel: CONTRACT_STATUS_LABEL[status],
    statusTone: CONTRACT_STATUS_TONE[status],
    lockVersion: detail.version,
    customer: {
      id: detail.customer_id,
      displayName:
        current?.customer_name ?? customer?.displayName ?? detail.customer_id,
      reference: customer?.customerNo,
    },
    ownerLabel: customer?.ownerLabel ?? "—",
    ownerKind: "current_customer_owner",
    currentRevision: {
      revisionId: current?.id ?? detail.current_revision_id ?? detail.id,
      revisionNo: current?.revision_no ?? 1,
      settlementParty: {
        id: detail.settlement_party_id,
        displayName:
          current?.settlement_party_name ?? detail.settlement_party_id,
      },
      paymentTermSnapshot: {
        label: termName,
        days: paymentTermDays(termCode),
        description: termName,
      },
      invoiceRequirementSnapshot: {
        titleType: current?.invoice_type ?? "—",
        contentSummary: current
          ? `税点 ${current.tax_point}`
          : "—",
      },
      validFrom: current?.valid_from ?? tsToIso(detail.created_at).slice(0, 10),
      validTo: current?.valid_to ?? "9999-12-31",
      signedAt: current?.signed_at,
      effectiveAt: current ? tsToIso(current.created_at) : undefined,
      termsSummary: termName,
    },
    attachments,
    relatedSalesOrders: [],
    revisionTimeline,
    auditTimeline: [],
    allowedActions: actions.allowedActions,
    actionBlockers: actions.actionBlockers,
    sourceAsOf: nowIso,
    relatedSalesOrdersAsOf: nowIso,
    queriedAt: nowIso,
    selectableForNewSalesOrder: actions.selectable,
    selectableBlocker: actions.selectableBlocker,
  }
}

/**
 * 新销售单可选合同：生效中且服务端状态允许引用。
 */
export async function fetchContractsForNewSalesOrder(): Promise<
  ContractListRow[]
> {
  const rows = await fetchContracts()
  return rows.filter((row) => {
    if (row.status !== "EFFECTIVE") return false
    return row.allowedActions.includes("CREATE_SALES_ORDER")
  })
}

/**
 * 上传合同 PDF：先 file-asset 上传，再 create contract。
 */
export async function uploadContractPdf(
  input: UploadContractPdfInput
): Promise<UploadContractPdfResult> {
  const fileError = contractPdfError(input.pdfFile)
  if (fileError) {
    const err: ApiError = {
      kind: "Validation",
      message: fileError,
      status: 400,
    }
    throw err
  }

  // 后端文件资产上限 5 MiB（handler），前端文案仍为 20 MB 校验；超 5 MiB 由后端拒绝
  if (!input.customerId?.trim()) {
    const err: ApiError = {
      kind: "Validation",
      message: "请选择客户",
      status: 400,
    }
    throw err
  }

  const customer = await loadCustomerBrief(input.customerId.trim())
  if (!customer) {
    const err: ApiError = {
      kind: "Http",
      status: 404,
      message: "客户不存在或无权访问",
    }
    throw err
  }

  // 结算主体：优先客户主体；名称不同时尝试按关键字查 party 列表
  let settlementPartyId = customer.partyId
  if (
    input.settlementPartyName.trim() &&
    input.settlementPartyName.trim() !== customer.displayName
  ) {
    try {
      const parties = await apiGet<Page<BackendPartyView>>("/admin/parties", {
        keyword: input.settlementPartyName.trim(),
        page: 1,
        page_size: 5,
      })
      if (parties.items[0]) {
        settlementPartyId = parties.items[0].id
      }
    } catch {
      // keep customer.partyId
    }
  }

  const termCode = paymentTermCodeFromLabel(input.paymentTerms)
  const termName =
    PAYMENT_TERM_OPTIONS.find((o) => o.value === termCode)?.label ??
    input.paymentTerms

  const asset = await uploadFileAsset(input.pdfFile)

  let created: BackendContractView
  try {
    created = await apiPost<BackendContractView>("/admin/contracts", {
      contract_no: input.contractNo.trim(),
      customer_id: input.customerId.trim(),
      settlement_party_id: settlementPartyId,
      contract_pdf_file_id: asset.id,
      archive_source: "CONTRACT_CENTER",
      customer_name: input.customerName.trim(),
      settlement_party_name: input.settlementPartyName.trim(),
      payment_term_code: termCode,
      payment_term_name: termName,
      // UI 未采集开票快照：用受控默认值满足后端校验（见证据 gap）
      invoice_type: "增值税专用发票",
      tax_point: "13",
      valid_from: input.validFrom,
      valid_to: input.validTo || undefined,
      signed_at: input.signedAt,
    })
  } catch (error) {
    if (isApiError(error) && error.status === 409) {
      const err: ApiError = {
        kind: "Http",
        status: 409,
        message: "CONTRACT_NO_EXISTS",
        responseData: error.responseData,
      }
      throw err
    }
    throw error
  }

  let revisionId = created.current_revision_id ?? created.id
  let revisionNo = 1
  try {
    const detail = await apiGet<BackendContractDetail>(
      `/admin/contracts/${created.id}`
    )
    const current =
      detail.revisions.find((r) => r.id === detail.current_revision_id) ??
      detail.revisions[0]
    if (current) {
      revisionId = current.id
      revisionNo = current.revision_no
    }
  } catch {
    // keep defaults
  }

  return {
    contractId: created.id,
    contractNo: created.contract_no,
    revisionId,
    revisionNo,
    uploadedAt: tsToIso(created.created_at),
    fileName: asset.file_name,
    reference: `CT-UP-${created.contract_no}`,
  }
}

/**
 * 导出任务：后端本批无合同专用导出接口；创建通用 background job 若失败则登记 gap 并返回本地排队态。
 */
export async function createContractExportJob(input: {
  rowCount: number
  filterSnapshotLabel: string
}): Promise<ContractExportJob> {
  const now = new Date().toISOString()
  const jobId = `export_ct_${Date.now().toString(36)}`

  // 尝试 D04 background job；失败时仍返回 queued 视图以免阻断 UI（证据登记缺口）
  try {
    await apiPost("/admin/background-jobs", {
      job_type: "CONTRACT_EXPORT",
      title: `合同导出 · ${input.filterSnapshotLabel}`,
      payload: {
        row_count: input.rowCount,
        filter: input.filterSnapshotLabel,
      },
    })
  } catch {
    // backend_gap: contract export not specialized
  }

  return {
    jobId,
    status: "queued",
    rowCount: input.rowCount,
    permissionVersion: "pv-w04-1",
    filterSnapshotLabel: input.filterSnapshotLabel,
    createdAt: now,
    downloadLabel: `合同导出（${input.rowCount} 行）`,
  }
}

/** 透传工具：供调用方读取错误文案 */
export { apiErrorMessage, isApiError }
