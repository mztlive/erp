import { redirect } from "next/navigation"

function serializeSearchParams(
    raw: Record<string, string | string[] | undefined>,
): string {
    const params = new URLSearchParams()
    for (const [key, value] of Object.entries(raw)) {
        if (typeof value === "string") params.append(key, value)
        else if (Array.isArray(value)) {
            for (const entry of value) params.append(key, entry)
        }
    }
    const qs = params.toString()
    return qs ? `?${qs}` : ""
}

export default async function LegacySalesSelectionsRedirectPage({
    searchParams,
}: {
    searchParams: Promise<Record<string, string | string[] | undefined>>
}) {
    const qs = serializeSearchParams(await searchParams)
    redirect(`/sales/selection${qs}`)
}
