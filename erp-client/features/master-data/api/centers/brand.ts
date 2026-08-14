/** 品牌对象中心：品牌代码 + Logo 资产回显。 */

import type { ProductBrandDto } from "@/features/master-data/api/contracts"
import { mapBrandRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import { fetchFileAsset } from "@/features/master-data/api/media-assets"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerBrand(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<ProductBrandDto>(
        "/admin/product-brands",
        {},
    )
    const dto = items.find((b) => b.id === stableId)
    if (!dto) return null
    const row = mapBrandRow(dto)
    const logoAssetId = dto.logo_asset_id?.trim()
    const logoAsset = logoAssetId ? await fetchFileAsset(logoAssetId) : null
    const logoUrl = logoAsset?.public_url?.trim()
    return baseCenter("brands", row, {
        resourceFacts: [
            { label: "品牌代码", value: dto.brand_code },
            {
                label: "品牌 Logo",
                value: logoUrl && logoAsset ? logoAsset.file_name : "—",
            },
        ],
        mediaAssets:
            logoUrl && logoAsset
                ? {
                      logo: [
                          {
                              fileName: logoAsset.file_name,
                              assetId: logoAssetId!,
                              url: logoUrl,
                          },
                      ],
                  }
                : undefined,
    })
}
