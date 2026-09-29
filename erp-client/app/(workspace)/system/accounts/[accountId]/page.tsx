import type { Metadata } from "next"
import { AccountDetailPage } from "@/features/admin/pages/account-detail-page"
export const metadata: Metadata = { title: "人员资料" }
export default async function AccountDetailRoute({
    params,
}: {
    params: Promise<{ accountId: string }>
}) {
    const { accountId } = await params
    return <AccountDetailPage accountId={accountId} />
}
