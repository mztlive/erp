import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

type SourceSystemDto = Readonly<{
    id: string
    code: string
    name: string
    system_type: string
    status: string
}>

export type MallComboboxItem = Readonly<{
    id: string
    code: string
    name: string
}>

export async function fetchMallOptions(): Promise<readonly MallComboboxItem[]> {
    const page = await apiGet<Page<SourceSystemDto>>("/admin/source-systems", {
        system_type: "MALL",
        status: "active",
        page: 1,
        page_size: 100,
        sort_by: "name",
        sort_dir: "asc",
    })
    return page.items.map((row) => ({
        id: row.id,
        code: row.code,
        name: row.name,
    }))
}
