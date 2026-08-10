import { redirect } from "next/navigation"

export default async function MasterDataIndexPage() {
    redirect("/master-data/sellable-items")
}
