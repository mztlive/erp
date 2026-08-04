import { mockDelay } from "@/features/workspace-kit/delay"
import { MOCK_SENSITIVE_REVEALS } from "@/features/master-data/data"
import {
  buildW14ListResult,
  createW14Object,
  disableW14Object,
  getW14Center,
  getW14SkuRevisions,
  queryW14Idempotency,
  reviseW14Object,
} from "@/features/master-data/session"
import type {
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  MasterDataCenterView,
  MasterDataListQuery,
  MasterDataListResult,
  MasterDataMutationResult,
  MasterDataResource,
  SkuRevisionRecord,
} from "@/features/master-data/types"
import { filterMasterDataRows } from "@/features/master-data/filter"

export async function fetchMasterDataList(
  query: MasterDataListQuery
): Promise<MasterDataListResult> {
  await mockDelay(90)
  const full = buildW14ListResult(query.resource)
  const rows = filterMasterDataRows(full.rows, query)
  return {
    ...full,
    rows,
    totalCount: rows.length,
  }
}

export async function fetchMasterDataCenter(
  resource: MasterDataResource,
  stableId: string
): Promise<MasterDataCenterView | null> {
  await mockDelay(100)
  return getW14Center(resource, stableId)
}

export async function createMasterDataObject(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  await mockDelay(120)
  return createW14Object(input)
}

export async function createMasterDataRevision(
  input: CreateRevisionInput
): Promise<MasterDataMutationResult> {
  await mockDelay(120)
  return reviseW14Object(input)
}

export async function disableMasterDataObject(
  input: DisableMasterDataInput
): Promise<MasterDataMutationResult> {
  await mockDelay(120)
  return disableW14Object(input)
}

export async function queryMasterDataIdempotency(
  idempotencyKey: string
): Promise<MasterDataMutationResult | null> {
  await mockDelay(60)
  return queryW14Idempotency(idempotencyKey)
}

/** 读取公司 SKU 修订（mock 落库的 `sku_revision.sales_visible_price_gross`）。 */
export async function fetchW14SkuRevisions(
  skuId: string
): Promise<readonly SkuRevisionRecord[] | undefined> {
  await mockDelay(40)
  return getW14SkuRevisions(skuId)
}

export async function revealMasterDataSensitive(
  revealToken: string
): Promise<string> {
  await mockDelay(80)
  const value = MOCK_SENSITIVE_REVEALS[revealToken]
  if (!value) {
    throw new Error("无权查看或权限已失效")
  }
  return value
}

/** Export uses current filter snapshot; permission re-check is mock copy. */
export function buildMasterDataExportCsv(
  rows: readonly {
    stableNo: string
    name: string
    revisionNo: number
    lifecycleStatusLabel: string
    revisionTimingLabel: string
    effectiveFrom: string
    effectiveTo?: string
    primaryBlocker?: string
  }[],
  filterSnapshotLabel: string,
  permissionVersion: string
): string {
  const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
  const header = [
    "资料编号",
    "名称",
    "版本",
    "启用状态",
    "版本状态",
    "生效开始",
    "生效结束",
    "不可用原因",
  ]
    .map(quote)
    .join(",")
  const body = rows
    .map((row) =>
      [
        row.stableNo,
        row.name,
        `v${row.revisionNo}`,
        row.lifecycleStatusLabel,
        row.revisionTimingLabel,
        row.effectiveFrom,
        row.effectiveTo ?? "长期",
        row.primaryBlocker ?? "",
      ]
        .map((v) => quote(v))
        .join(",")
    )
    .join("\n")
  const meta = [
    `# 筛选条件=${filterSnapshotLabel}`,
    `# 权限核对=${permissionVersion}`,
    `# 说明=导出时按权限重新核对；不含无权查看的敏感信息`,
  ].join("\n")
  return `${meta}\n${header}\n${body}`
}

export function downloadCsv(content: string, fileName: string) {
  const url = URL.createObjectURL(
    new Blob(["\uFEFF", content], { type: "text/csv;charset=utf-8" })
  )
  const anchor = document.createElement("a")
  anchor.href = url
  anchor.download = fileName.endsWith(".csv") ? fileName : `${fileName}.csv`
  anchor.click()
  URL.revokeObjectURL(url)
}
