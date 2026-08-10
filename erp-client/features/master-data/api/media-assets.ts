import { apiGet } from "@/lib/api"
import type { BackendFileAsset } from "@/features/master-data/api/contracts"

/** 按文件资产 ID 查询详情（含公开访问地址，供媒体回显）。 */
export async function fetchFileAsset(assetId: string): Promise<BackendFileAsset | null> {
  try {
    return await apiGet<BackendFileAsset>(`/admin/file-assets/${encodeURIComponent(assetId)}`)
  } catch {
    return null
  }
}

/** 批量解析媒体文件资产为 `assetId → 资产详情`（去重，单条失败降级为缺失）。 */
export async function resolveMediaAssets(
  assetIds: readonly string[],
): Promise<Map<string, BackendFileAsset>> {
  const unique = [...new Set(assetIds.filter((id) => id.trim()))]
  const resolved = new Map<string, BackendFileAsset>()
  for (const assetId of unique) {
    const asset = await fetchFileAsset(assetId)
    if (asset) resolved.set(assetId, asset)
  }
  return resolved
}
